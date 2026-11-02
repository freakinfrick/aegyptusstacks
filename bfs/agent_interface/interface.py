"""L6 Agent Interface — Unified Entry Point with Tier Enforcement.

The three-tier separation is a CORE DESIGN GUARANTEE. Agents cannot
fabricate locations because Tier 2/3 operations require coordinates
produced by Tier 1. Mixing tiers within a single call is prohibited.
"""

from __future__ import annotations

from typing import Any

from bfs.agent_interface.tier1_retrieval import RetrievalTools
from bfs.agent_interface.tier2_spatial import SpatialTools
from bfs.agent_interface.tier3_narration import NarrationTools


# Methods that accept coordinate parameters (q, r, z).
# These must be validated for Tier 2 and Tier 3 calls.
_COORD_PARAMS = frozenset({"q", "r", "z", "from_q", "from_r", "from_z", "to_q", "to_r", "to_z"})


class AgentInterface:
    """Unified entry point for all agent tool calls.

    Enforces tier separation:
    - Tier 1: Pure reads. No validation needed.
    - Tier 2: Session mutations. Coordinate params validated against topology.
    - Tier 3: Overlay mutations. Coordinate params validated against topology.
    """

    def __init__(
        self,
        retrieval: RetrievalTools,
        spatial: SpatialTools,
        narration: NarrationTools,
    ) -> None:
        self.tier1 = retrieval
        self.tier2 = spatial
        self.tier3 = narration
        self._tiers: dict[int, Any] = {
            1: self.tier1,
            2: self.tier2,
            3: self.tier3,
        }

    def execute(self, tier: int, method: str, **params: Any) -> dict[str, Any]:
        """Execute a tool call with tier validation.

        Tier 2/3: validates coordinate params exist in topology before dispatch.
        """
        tier_obj = self._tiers.get(tier)
        if tier_obj is None:
            raise ValueError(f"Invalid tier: {tier}")

        handler = getattr(tier_obj, method, None)
        if handler is None or method.startswith("_"):
            raise ValueError(f"Unknown method: {method} in tier {tier}")

        # Validate coordinates for Tier 2 and 3
        if tier in (2, 3):
            self._validate_coord_params(params)

        return handler(**params)

    def validate_coordinate(self, q: int, r: int, z: int) -> bool:
        """Check that a coordinate exists in the resolved topology."""
        return self.tier2._validate_coord(q, r, z)

    def list_methods(self, tier: int) -> list[str]:
        """List available methods for a tier."""
        tier_obj = self._tiers.get(tier)
        if tier_obj is None:
            raise ValueError(f"Invalid tier: {tier}")
        return [
            name
            for name in dir(tier_obj)
            if not name.startswith("_") and callable(getattr(tier_obj, name))
        ]

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _validate_coord_params(self, params: dict[str, Any]) -> None:
        """Validate any coordinate parameters in the call.

        Groups (q, r, z), (from_q, from_r, from_z), (to_q, to_r, to_z)
        and validates each complete triple against the topology.
        """
        # Check for direct (q, r, z) triple
        if all(k in params for k in ("q", "r", "z")):
            q, r, z = params["q"], params["r"], params["z"]
            if not self.tier2._validate_coord(q, r, z):
                raise ValueError(f"Invalid coordinate: ({q},{r},{z})")

        # Check for from_ triple
        if all(k in params for k in ("from_q", "from_r", "from_z")):
            q, r, z = params["from_q"], params["from_r"], params["from_z"]
            if not self.tier2._validate_coord(q, r, z):
                raise ValueError(f"Invalid source coordinate: ({q},{r},{z})")

        # Check for to_ triple
        if all(k in params for k in ("to_q", "to_r", "to_z")):
            q, r, z = params["to_q"], params["to_r"], params["to_z"]
            if not self.tier2._validate_coord(q, r, z):
                raise ValueError(f"Invalid destination coordinate: ({q},{r},{z})")
