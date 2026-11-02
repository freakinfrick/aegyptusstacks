"""L7 Runtime State Manager — Redis-backed session persistence and wear tracking."""

from __future__ import annotations

import math
from datetime import datetime, timezone

import redis

from .session import SessionState, TraversalEntry, WorkspaceCopy

ENTRANCE_COORD = (0, 0, 3)  # Observatory — top of the library
WORKSPACE_COORD = (999, 999, 0)  # User's personal study, outside the main grid
SESSION_TTL_SECONDS = 86400  # 24 hours
MAX_TRAVERSAL_HISTORY = 500  # cap unbounded growth


class StateManager:
    """Manages session state via Redis.

    Sessions are stored as JSON under ``session:{session_id}`` keys.
    """

    def __init__(self, redis_url: str = "redis://localhost:6379") -> None:
        self.redis = redis.Redis.from_url(redis_url, decode_responses=True)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def create_session(
        self,
        start_position: tuple[int, int, int] = ENTRANCE_COORD,
    ) -> SessionState:
        """Create a new session starting at *start_position*."""
        session = SessionState(
            current_position=start_position,
        )
        # Record the starting room as the first traversal entry
        session.traversal_history.append(
            TraversalEntry(
                coord=start_position,
                entered_at=session.created_at,
                entered_via="teleport",
            )
        )
        session.rooms_visited.append(start_position)
        self._persist(session)
        return session

    def get_session(self, session_id: str) -> SessionState | None:
        """Load session from Redis.  Returns ``None`` if not found."""
        return self._load(session_id)

    def update_position(
        self,
        session_id: str,
        new_pos: tuple[int, int, int],
        via: str = "walk",
    ) -> SessionState:
        """Move user to *new_pos*, record traversal, update wear."""
        session = self._load_or_raise(session_id)
        now = datetime.now(timezone.utc)

        # Close previous traversal entry
        if session.traversal_history:
            last = session.traversal_history[-1]
            if last.exited_at is None:
                last.exited_at = now
                last.dwell_time_seconds = (
                    last.exited_at - last.entered_at
                ).total_seconds()

        # Open new entry
        session.traversal_history.append(
            TraversalEntry(coord=new_pos, entered_at=now, entered_via=via)
        )
        session.current_position = new_pos

        if new_pos not in session.rooms_visited:
            session.rooms_visited.append(new_pos)

        session.current_volume = None
        session.current_shelf = None
        session.last_active = now
        session.interaction_count += 1
        self._persist(session)
        return session

    def add_to_workspace(
        self,
        session_id: str,
        canonical_id: str,
        source_coord: tuple[int, int, int],
    ) -> SessionState:
        """Gather a volume copy to the session workspace."""
        session = self._load_or_raise(session_id)
        copy = WorkspaceCopy(
            canonical_id=canonical_id,
            source_coord=source_coord,
            fetched_at=datetime.now(timezone.utc),
        )
        session.workspace_volumes.append(copy)
        session.last_active = datetime.now(timezone.utc)
        session.interaction_count += 1
        self._persist(session)
        return session

    def get_workspace(self, session_id: str) -> list[WorkspaceCopy]:
        """Return the workspace contents for a session."""
        session = self._load_or_raise(session_id)
        return session.workspace_volumes

    def add_bookmark(
        self,
        session_id: str,
        label: str,
        coord: tuple[int, int, int],
    ) -> SessionState:
        """Bookmark a room with a human-readable label."""
        session = self._load_or_raise(session_id)
        session.bookmarks[label] = coord
        session.last_active = datetime.now(timezone.utc)
        session.interaction_count += 1
        self._persist(session)
        return session

    # ------------------------------------------------------------------
    # Persistence helpers
    # ------------------------------------------------------------------

    def _persist(self, session: SessionState) -> None:
        """Save session to Redis as JSON with TTL and history cap."""
        # Cap traversal history to prevent unbounded growth
        if len(session.traversal_history) > MAX_TRAVERSAL_HISTORY:
            session.traversal_history = session.traversal_history[-MAX_TRAVERSAL_HISTORY:]
        self.redis.set(
            f"session:{session.session_id}",
            session.model_dump_json(),
            ex=SESSION_TTL_SECONDS,
        )

    def _load(self, session_id: str) -> SessionState | None:
        data: str | None = self.redis.get(f"session:{session_id}")  # type: ignore[assignment]
        if data:
            return SessionState.model_validate_json(data)
        return None

    def _load_or_raise(self, session_id: str) -> SessionState:
        session = self._load(session_id)
        if session is None:
            msg = f"Session not found: {session_id}"
            raise KeyError(msg)
        return session


class WearTracker:
    """Aggregate visit counts across all sessions for aesthetic wear effects.

    Heavily-visited rooms show wear: darkened floor stones, brighter candles,
    fewer cobwebs.  The wear value is a log-scaled float in ``[0.0, 1.0]``.
    """

    def __init__(self, redis_url: str = "redis://localhost:6379") -> None:
        self.redis = redis.Redis.from_url(redis_url, decode_responses=True)

    # Lua script: atomically increment visit count and update max
    _VISIT_LUA = """
    local current = redis.call('HINCRBY', KEYS[1], ARGV[1], 1)
    local max_val = tonumber(redis.call('GET', KEYS[2]) or 0) or 0
    if current > max_val then
        redis.call('SET', KEYS[2], current)
    end
    return current
    """

    def record_visit(self, coord: tuple[int, int, int]) -> None:
        """Increment the global visit counter for a room (atomic)."""
        key = f"{coord[0]}:{coord[1]}:{coord[2]}"
        if not hasattr(self, "_visit_script"):
            self._visit_script = self.redis.register_script(self._VISIT_LUA)
        self._visit_script(keys=["wear:visits", "wear:max"], args=[key])

    def get_wear(self, coord: tuple[int, int, int]) -> float:
        """Return wear factor: ``0.0`` = never visited, ``1.0`` = heavily worn."""
        key = f"{coord[0]}:{coord[1]}:{coord[2]}"
        visits = int(self.redis.hget("wear:visits", key) or 0)  # type: ignore[arg-type]
        max_visits = int(self.redis.get("wear:max") or 1)  # type: ignore[arg-type]
        if max_visits <= 0:
            return 0.0
        return math.log(visits + 1) / math.log(max_visits + 1)
