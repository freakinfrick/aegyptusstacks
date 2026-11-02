mod game;
mod render;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn capitalize_type(s: &str) -> String {
    match s {
        "scroll" => "Scroll".into(),
        "stele" | "rounded_stele" | "rectangular_stele" => "Stele".into(),
        "papyrus" => "Papyrus".into(),
        "vessel" | "object" => "Vessel".into(),
        "coffin" => "Coffin".into(),
        "sculpture" => "Sculpture".into(),
        "ostrakon" | "ostracon" => "Ostrakon".into(),
        "tomb" => "Tomb".into(),
        "wall_inscription" | "wall" => "Wall Inscription".into(),
        "false_door" | "doorway" => "False Door".into(),
        "rock" | "rock_cut" => "Rock Inscription".into(),
        "unknown" | "undetermined" => "Text".into(),
        "writing_tablet" | "writing_board" | "writing_medium" => "Writing Tablet".into(),
        other => {
            let mut c = other.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}
use std::time::Instant;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use game::{ActionResult, ActorKind, RoomType, RoomComposition};
use game::{era_name_from_z, display_district, atmospheric_text, composition_summary, room_name_from_cluster};
use game::input::map_key_to_action;
use game::world::WorldMap;
use render::room_view::RoomView;
use render::lighting::collect_light_sources;
use render::particles::{RoomParticles, init_room_particles, tick_particles};
use render::palette::{Palette, Tier};
use render::popup::{PopupContent, render_popup, render_chat_sidebar};
use render::hud::{render_hud, HudData, HudMode};
use bfs_client::{BfsClient, RoomMetadata, VolumeContent};
use game::hermes::{HermesBridge, HermesEvent, ChatMessage, ChatRole, ChatStatus, ContextHeader, TokenBudget};
use game::inventory::Inventory;
use game::scribe::{ScribeBridge, ScribeEvent, ScribeStatus};
use render::gather_vortex::{GatherVortex, VortexStatus};

struct ChatState {
    messages: Vec<ChatMessage>,
    input_buffer: String,
    input_cursor: usize,
    scroll_offset: usize,
}

/// Result from a background preload task
enum PreloadResult {
    Room(RoomMetadata),
    Miss([i32; 3]),
    Done,
}

struct App {
    engine: game::Engine,
    bfs: BfsClient,
    world: WorldMap,
    should_quit: bool,
    status_message: Option<String>,
    active_popup: Option<PopupContent>,
    current_palette: Palette,
    /// Atmospheric entry text (line 3) with expiry
    atmosphere_text: Option<String>,
    atmosphere_expires: Option<Instant>,
    /// Current room composition summary cached for status bar
    composition_cache: String,
    /// Shelf reading context: (canonical_ids, physical_types, current_index) for flipping through texts
    shelf_context: Option<(Vec<String>, Vec<String>, usize)>,
    /// Saved popup to restore when returning from LecternRead (Esc goes back)
    saved_popup: Option<PopupContent>,
    /// Cached volumes for search result browsing (cleared on new search)
    volume_cache: HashMap<String, VolumeContent>,
    /// Label for shelf_context source: "search", "collection", etc. (shown in lectern header)
    shelf_source_label: Option<String>,
    /// HUD overlay mode: Off → Standard → Full (with legend) → Off
    hud_mode: HudMode,
    /// Navigation breadcrumb: last N rooms visited
    breadcrumb_trail: VecDeque<[i32; 3]>,
    hermes_bridge: Option<HermesBridge>,
    /// Background task spawning the hermes bridge (cold start)
    bridge_pending: Option<tokio::task::JoinHandle<Result<HermesBridge, String>>>,
    interaction_count: u32,
    token_budget: TokenBudget,
    /// Persistent chat state — survives panel open/close
    chat_state: Option<ChatState>,
    /// Sidebar streaming state — updated even when popup is closed
    sidebar_streaming: String,
    sidebar_status: ChatStatus,
    sidebar_status_text: Option<String>,
    /// Global animation tick counter
    tick: u64,
    /// Per-room particle state
    room_particles: HashMap<[i32; 3], RoomParticles>,
    /// Hermes model/connection info for chat border
    hermes_model: String,
    hermes_endpoint: String,
    /// Player inventory of gathered texts
    inventory: Inventory,
    /// Scribe synthesis bridge
    scribe_bridge: Option<ScribeBridge>,
    scribe_streaming: String,
    scribe_status: ScribeStatus,
    scribe_status_text: Option<String>,
    /// Ring buffer of reasoning lines (last ~50, renderer shows tail)
    scribe_reasoning_ring: Vec<String>,
    /// Ring buffer of hermes reasoning deltas for chat status display
    chat_reasoning_buf: String,
    /// Negative cache: coords confirmed absent by BFS (avoids re-fetching voids)
    bfs_miss_cache: HashSet<[i32; 3]>,
    /// Background corpus search task
    search_pending: Option<tokio::task::JoinHandle<Vec<render::popup::CorpusSearchHit>>>,
    /// Stacking gather toast messages — (text, glyph, color, expires_at)
    gather_toasts: Vec<(String, &'static str, ratatui::style::Color, Instant)>,
    /// Background preload: receiver for rooms fetched by spawned task
    preload_rx: tokio::sync::mpsc::Receiver<PreloadResult>,
    /// Sender cloned into spawned preload tasks
    preload_tx: tokio::sync::mpsc::Sender<PreloadResult>,
    /// Whether a preload task is currently in flight
    preload_in_flight: bool,
    /// Command server: receives game commands from stacks-game.py via Unix socket
    command_rx: tokio::sync::mpsc::Receiver<game::CommandRequest>,
    /// Scribe summoning ritual ring (rendering overlay)
    ritual_ring: Option<render::ritual::RitualRing>,
    /// Active gather vortex animations (deferred toasts fire on completion)
    gather_vortices: Vec<GatherVortex>,
}

impl App {
    async fn new() -> Self {
        let bfs = BfsClient::new("http://localhost:8081");
        let mut world = WorldMap::new();
        let mut engine = game::Engine::new();
        let current_palette = Palette::from_tier(Tier::Surface);
        let (tx, rx) = tokio::sync::mpsc::channel::<PreloadResult>(128);
        let command_rx = game::command_server::start();
        let mut status = Some("Loading world...".to_string());

        // Start at the observatory, which connects to the main shaft column.
        // The shaft descends through all z-levels at the same (q,r).
        let start_coords: &[[i32; 3]] = &[
            [0, 0, 3],       // Observatory — shaft down to main column
            [4, -16, 0],     // Main shaft column top (fallback)
            [-5, 12, -1],
        ];

        let mut rooms_loaded = 0;
        for start in start_coords {
            if let Ok(meta) = bfs.get_room(start[0], start[1], start[2]).await {
                // BFS-walk collecting up to 40 rooms (includes z-adjacent via shafts)
                let collected = bfs_collect_rooms(&bfs, &meta, 40).await;
                let mut first_center = None;
                for room_meta in &collected {
                    let center = world.place_room(room_meta);
                    if first_center.is_none() {
                        first_center = Some(center);
                    }
                    rooms_loaded += 1;
                }
                engine.current_room = [meta.q, meta.r, meta.z];
                // Spawn player offset from center
                if let Some(center) = first_center {
                    let spawn = center + hex2d_stacks::Coordinate::<i32>::from(
                        hex2d_stacks::Direction::XY
                    ).scale(3);
                    engine.player.pos.coord = spawn;

                    // Spawn librarian 2 hexes behind player
                    let lib_dir = engine.player.facing() + hex2d_stacks::Angle::Back;
                    let lib_target = spawn + lib_dir + lib_dir;
                    let lib_pos = if world.tiles.get(&lib_target)
                        .map(|wt| wt.tile.is_passable()).unwrap_or(false)
                    {
                        lib_target
                    } else {
                        spawn // fallback: same tile as player
                    };
                    let librarian = game::Actor::new(
                        game::ActorKind::Librarian,
                        lib_pos,
                        engine.player.facing(),
                    );
                    engine.librarians.push(librarian);
                }
                status = None;
                break;
            }
        }

        if rooms_loaded == 0 {
            status = Some("BFS: could not load any rooms".to_string());
        }

        // Stamp article gallery rooms around the observatory
        let article_entries: Vec<(String, String)> = Self::scan_articles()
            .into_iter()
            .map(|a| (a.filename, a.title))
            .collect();
        if !article_entries.is_empty() {
            world.stamp_article_galleries(article_entries);
        }

        // Set palette from actual starting room (overrides the generic Surface default)
        let current_palette = world.rooms.get(&engine.current_room)
            .map(|r| Palette::from_room(r.info.z, &r.info.district))
            .unwrap_or(current_palette);

        // Initial composition cache for starting room
        let initial_comp = world.rooms.get(&engine.current_room)
            .map(|r| {
                let comp = RoomComposition::from_volumes_typed(&r.info.volumes_typed);
                composition_summary(&comp)
            })
            .unwrap_or_default();

        App {
            engine,
            bfs,
            world,
            should_quit: false,
            status_message: status,
            active_popup: None,
            current_palette,
            atmosphere_text: None,
            atmosphere_expires: None,
            composition_cache: initial_comp,
            shelf_context: None,
            saved_popup: None,
            volume_cache: HashMap::new(),
            shelf_source_label: None,
            hud_mode: HudMode::Standard,
            breadcrumb_trail: VecDeque::with_capacity(8),
            hermes_bridge: None,
            bridge_pending: None,
            interaction_count: 0,
            token_budget: TokenBudget::new(),
            chat_state: None,
            sidebar_streaming: String::new(),
            sidebar_status: ChatStatus::Idle,
            sidebar_status_text: None,
            tick: 0,
            room_particles: HashMap::new(),
            hermes_model: String::new(),
            hermes_endpoint: "local".to_string(),
            inventory: Inventory::new(),
            scribe_bridge: None,
            scribe_streaming: String::new(),
            scribe_status: ScribeStatus::Idle,
            scribe_status_text: None,
            scribe_reasoning_ring: Vec::new(),
            chat_reasoning_buf: String::new(),
            bfs_miss_cache: HashSet::new(),
            search_pending: None,
            gather_toasts: Vec::new(),
            preload_rx: rx,
            preload_tx: tx,
            preload_in_flight: false,
            command_rx,
            ritual_ring: None,
            gather_vortices: Vec::new(),
        }
    }

    async fn handle_key(&mut self, key: KeyCode) {
        // Chat panel — MODAL: locks all input (like lectern)
        if let Some(PopupContent::Chat {
            ref mut messages,
            ref mut input_buffer,
            ref mut input_cursor,
            ref mut status,
            ref mut scroll_offset,
            ref mut streaming_partial,
            ref mut status_text,
            ..
        }) = self.active_popup {
            match key {
                KeyCode::Esc => {
                    // Minimize: save state, close modal, switch to sidebar
                    let saved_messages = messages.clone();
                    let saved_buffer = std::mem::take(input_buffer);
                    let saved_cursor = *input_cursor;
                    let saved_scroll = *scroll_offset;
                    self.chat_state = Some(ChatState {
                        messages: saved_messages,
                        input_buffer: saved_buffer,
                        input_cursor: saved_cursor,
                        scroll_offset: saved_scroll,
                    });
                    self.active_popup = None;
                    self.hud_mode = HudMode::ChatSidebar;
                    return;
                }
                KeyCode::Enter => {
                    if matches!(status, ChatStatus::Idle) && !input_buffer.trim().is_empty() {
                        if self.hermes_bridge.is_none() {
                            // Bridge not connected — inform user instead of silently dropping
                            messages.push(ChatMessage::new(
                                ChatRole::System,
                                if self.bridge_pending.is_some() {
                                    "Librarian connecting... please wait.".to_string()
                                } else {
                                    "Librarian unavailable.".to_string()
                                },
                            ));
                        } else {
                            let text = std::mem::take(input_buffer);
                            *input_cursor = 0;
                            *scroll_offset = 0;
                            messages.push(ChatMessage::new(ChatRole::User, text.clone()));
                            *status = ChatStatus::WaitingForAgent;
                            self.interaction_count += 1;

                            let ctx = ContextHeader::build(&self.engine, &self.world, self.interaction_count, &self.inventory.ids());
                            if let Some(ref mut bridge) = self.hermes_bridge {
                                let _ = bridge.send_message(&text, &ctx).await;
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    if matches!(status, ChatStatus::Idle) && *input_cursor > 0 {
                        // Step back to previous char boundary
                        let prev = input_buffer[..*input_cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        input_buffer.remove(prev);
                        *input_cursor = prev;
                    }
                }
                KeyCode::Delete => {
                    if matches!(status, ChatStatus::Idle) && *input_cursor < input_buffer.len() {
                        input_buffer.remove(*input_cursor);
                    }
                }
                KeyCode::Left => {
                    if matches!(status, ChatStatus::Idle) && *input_cursor > 0 {
                        *input_cursor = input_buffer[..*input_cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                }
                KeyCode::Right => {
                    if matches!(status, ChatStatus::Idle) && *input_cursor < input_buffer.len() {
                        *input_cursor = input_buffer[*input_cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| *input_cursor + i)
                            .unwrap_or(input_buffer.len());
                    }
                }
                KeyCode::Home => {
                    if matches!(status, ChatStatus::Idle) {
                        *input_cursor = 0;
                    }
                }
                KeyCode::End => {
                    if matches!(status, ChatStatus::Idle) {
                        *input_cursor = input_buffer.len();
                    }
                }
                // Scroll: Up/Down
                KeyCode::Up => {
                    *scroll_offset = scroll_offset.saturating_add(1);
                }
                KeyCode::Down => {
                    *scroll_offset = scroll_offset.saturating_sub(1);
                }
                // Typing: only when idle
                KeyCode::Char(c) => {
                    if matches!(status, ChatStatus::Idle) {
                        input_buffer.insert(*input_cursor, c);
                        *input_cursor += c.len_utf8();
                    }
                }
                _ => {} // All other keys consumed (modal)
            }
            return; // Chat consumes ALL input — no fall-through
        }

        // ScribeChat: non-modal during writing, scrollable after complete
        if matches!(&self.active_popup, Some(PopupContent::ScribeChat { .. })) {
            match key {
                KeyCode::Esc => {
                    self.active_popup = None;
                    return;
                }
                KeyCode::Char(' ') => {
                    if matches!(self.scribe_status, ScribeStatus::Complete) {
                        self.active_popup = None;
                        return;
                    }
                    // During writing, fall through to game input
                }
                KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                    if matches!(self.scribe_status, ScribeStatus::Complete) {
                        if let Some(PopupContent::ScribeChat { ref mut scroll_offset, .. }) = self.active_popup {
                            *scroll_offset = scroll_offset.saturating_add(1);
                        }
                        return;
                    }
                }
                KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Down => {
                    if matches!(self.scribe_status, ScribeStatus::Complete) {
                        if let Some(PopupContent::ScribeChat { ref mut scroll_offset, .. }) = self.active_popup {
                            *scroll_offset = scroll_offset.saturating_sub(1);
                        }
                        return;
                    }
                }
                _ => {}
            }
            // During writing, fall through to game input (non-modal)
        }

        // Pause menu — modal
        if matches!(&self.active_popup, Some(PopupContent::PauseMenu { .. })) {
            use render::popup::PauseView;
            // Extract current view to decide dispatch
            let current_view = if let Some(PopupContent::PauseMenu { ref view, .. }) = self.active_popup {
                view.clone()
            } else { return; };

            match current_view {
                PauseView::Menu => {
                    match key {
                        KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                            if let Some(PopupContent::PauseMenu { ref mut selected, .. }) = self.active_popup {
                                if *selected > 0 { *selected -= 1; }
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Down => {
                            if let Some(PopupContent::PauseMenu { ref mut selected, .. }) = self.active_popup {
                                if *selected < 7 { *selected += 1; }
                            }
                        }
                        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                            // Enter inventory browser
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut inventory_selected, .. }) = self.active_popup {
                                *view = render::popup::PauseView::Inventory;
                                *inventory_selected = 0;
                            }
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            let sel = if let Some(PopupContent::PauseMenu { selected, .. }) = self.active_popup {
                                selected
                            } else { return; };
                            match sel {
                                0 => { self.active_popup = None; } // Resume
                                1 => { // Controls
                                    if let Some(PopupContent::PauseMenu { ref mut view, .. }) = self.active_popup {
                                        *view = PauseView::Controls;
                                    }
                                }
                                2 => { // Map
                                    if let Some(PopupContent::PauseMenu { ref mut view, ref mut map_center, .. }) = self.active_popup {
                                        *view = PauseView::Map;
                                        *map_center = self.engine.current_room;
                                    }
                                    self.spawn_preload_map_visible();
                                }
                                3 => { // Sessions
                                    if let Some(PopupContent::PauseMenu { ref mut view, ref mut sessions_loading, ref mut session_list, ref mut session_selected, .. }) = self.active_popup {
                                        *view = PauseView::Sessions;
                                        *sessions_loading = true;
                                        *session_list = Vec::new();
                                        *session_selected = 0;
                                    }
                                    if let Some(ref mut bridge) = self.hermes_bridge {
                                        match bridge.list_sessions().await {
                                            Ok(sessions) => {
                                                if let Some(PopupContent::PauseMenu { ref mut session_list, ref mut sessions_loading, .. }) = self.active_popup {
                                                    *session_list = sessions;
                                                    *sessions_loading = false;
                                                }
                                            }
                                            Err(e) => {
                                                if let Some(PopupContent::PauseMenu { ref mut sessions_loading, .. }) = self.active_popup {
                                                    *sessions_loading = false;
                                                }
                                                self.status_message = Some(format!("Session list failed: {}", e));
                                            }
                                        }
                                    } else {
                                        if let Some(PopupContent::PauseMenu { ref mut sessions_loading, .. }) = self.active_popup {
                                            *sessions_loading = false;
                                        }
                                        self.status_message = Some("Librarian bridge not connected".into());
                                    }
                                }
                                4 => { // Corpus Search
                                    if let Some(PopupContent::PauseMenu { ref mut view, ref mut search_input, ref mut search_results, ref mut search_selected, ref mut search_loading, .. }) = self.active_popup {
                                        *view = PauseView::CorpusSearch;
                                        search_input.clear();
                                        search_results.clear();
                                        *search_selected = 0;
                                        *search_loading = false;
                                    }
                                }
                                5 => { // Articles
                                    // Scan articles directory
                                    let articles = Self::scan_articles();
                                    if let Some(PopupContent::PauseMenu { ref mut view, ref mut article_list, ref mut article_selected, ref mut article_body, ref mut article_scroll, .. }) = self.active_popup {
                                        *view = PauseView::Articles;
                                        *article_list = articles;
                                        *article_selected = 0;
                                        article_body.clear();
                                        *article_scroll = 0;
                                    }
                                }
                                6 => { // Room Lookup
                                    if let Some(PopupContent::PauseMenu { ref mut view, ref mut coord_input, ref mut lookup_result, .. }) = self.active_popup {
                                        *view = PauseView::RoomLookup;
                                        coord_input.clear();
                                        *lookup_result = None;
                                    }
                                }
                                7 => { self.should_quit = true; } // Quit
                                _ => {}
                            }
                        }
                        KeyCode::Esc => {
                            self.active_popup = None; // Resume
                        }
                        _ => {}
                    }
                }
                PauseView::Controls => {
                    match key {
                        KeyCode::Esc => {
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                *view = PauseView::Menu;
                                *selected = 1;
                            }
                        }
                        _ => {}
                    }
                }
                PauseView::Inventory => {
                    match key {
                        KeyCode::Esc | KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                *view = PauseView::Menu;
                                *selected = 0;
                            }
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            if let Some(PopupContent::PauseMenu { ref mut inventory_selected, .. }) = self.active_popup {
                                if *inventory_selected > 0 { *inventory_selected -= 1; }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('x') | KeyCode::Char('X') => {
                            if let Some(PopupContent::PauseMenu { ref mut inventory_selected, ref inventory_items, .. }) = self.active_popup {
                                if *inventory_selected + 1 < inventory_items.len() { *inventory_selected += 1; }
                            }
                        }
                        KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                            // Open selected volume as LecternRead
                            let vol_id = if let Some(PopupContent::PauseMenu { ref inventory_items, ref inventory_selected, .. }) = self.active_popup {
                                inventory_items.get(*inventory_selected).map(|(id, _, _, _, _)| id.clone())
                            } else { None };
                            if let Some(id) = vol_id {
                                let saved = self.active_popup.clone();
                                // Set shelf_context so Left/Right can flip through inventory
                                let all_ids: Vec<String> = self.inventory.volumes.iter().map(|v| v.canonical_id.clone()).collect();
                                let all_types: Vec<String> = self.inventory.volumes.iter().map(|v| v.physical_type.clone()).collect();
                                let idx = all_ids.iter().position(|i| i == &id).unwrap_or(0);
                                self.shelf_context = Some((all_ids, all_types, idx));
                                self.shelf_source_label = None;
                                self.read_volume_at_index(idx).await;
                                if matches!(&self.active_popup, Some(PopupContent::LecternRead { .. })) {
                                    self.saved_popup = saved;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                PauseView::Map => {
                    let mut panned = false;
                    match key {
                        KeyCode::Esc => {
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                *view = PauseView::Menu;
                                *selected = 2;
                            }
                        }
                        // Pan: QWEDCXZA — BFS r+ = screen up, q+ = screen right
                        KeyCode::Char('w') | KeyCode::Char('W') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[1] += 1; // up = r+1
                            }
                            panned = true;
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[1] -= 1; // down = r-1
                            }
                            panned = true;
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[0] -= 1; // left = q-1
                            }
                            panned = true;
                        }
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[0] += 1; // right = q+1
                            }
                            panned = true;
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[0] -= 1; // NW = q-1, r+1
                                map_center[1] += 1;
                            }
                            panned = true;
                        }
                        KeyCode::Char('e') | KeyCode::Char('E') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[0] += 1; // NE = q+1, r+1
                                map_center[1] += 1;
                            }
                            panned = true;
                        }
                        KeyCode::Char('z') | KeyCode::Char('Z') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[0] -= 1; // SW = q-1, r-1
                                map_center[1] -= 1;
                            }
                            panned = true;
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[0] += 1; // SE = q+1, r-1
                                map_center[1] -= 1;
                            }
                            panned = true;
                        }
                        // Z-level: R up, F down
                        KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Up => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[2] += 1;
                            }
                            panned = true;
                        }
                        KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Down => {
                            if let Some(PopupContent::PauseMenu { ref mut map_center, .. }) = self.active_popup {
                                map_center[2] -= 1;
                            }
                            panned = true;
                        }
                        // Space: navigate player to this location
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            let coord = if let Some(PopupContent::PauseMenu { ref map_center, .. }) = self.active_popup {
                                *map_center
                            } else { return; };
                            self.active_popup = None;
                            self.navigate_to_room(coord).await;
                        }
                        _ => {}
                    }
                    if panned {
                        self.spawn_preload_map_visible();
                    }
                }
                PauseView::Sessions => {
                    match key {
                        KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                            if let Some(PopupContent::PauseMenu { ref mut session_selected, .. }) = self.active_popup {
                                if *session_selected > 0 { *session_selected -= 1; }
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Down => {
                            if let Some(PopupContent::PauseMenu { ref mut session_selected, ref session_list, .. }) = self.active_popup {
                                if *session_selected + 1 < session_list.len() { *session_selected += 1; }
                            }
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            // Resume selected session
                            let target_id = if let Some(PopupContent::PauseMenu { ref session_list, ref session_selected, .. }) = self.active_popup {
                                session_list.get(*session_selected).map(|s| s.session_id.clone())
                            } else { None };

                            if let Some(session_id) = target_id {
                                if let Some(ref mut bridge) = self.hermes_bridge {
                                    match bridge.resume_session(&session_id).await {
                                        Ok((messages, model, context_max)) => {
                                            // Update model display + token budget
                                            if let Some(m) = model {
                                                let short = m.rsplit('/').next().unwrap_or(&m)
                                                    .trim_end_matches(".gguf").to_string();
                                                self.hermes_model = short;
                                            }
                                            if context_max > 0 {
                                                self.token_budget.context_window = context_max;
                                            }
                                            // Close pause menu, open chat UI with loaded messages
                                            self.chat_state = Some(ChatState {
                                                messages: messages.clone(),
                                                input_buffer: String::new(),
                                                input_cursor: 0,
                                                scroll_offset: 0,
                                            });
                                            self.active_popup = Some(PopupContent::Chat {
                                                messages,
                                                input_buffer: String::new(),
                                                input_cursor: 0,
                                                streaming_partial: String::new(),
                                                streaming_is_thinking: false,
                                                status: game::hermes::ChatStatus::Idle,
                                                scroll_offset: 0,
                                                status_text: None,
                                            });
                                        }
                                        Err(e) => {
                                            self.status_message = Some(format!("Resume failed: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Esc => {
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                *view = PauseView::Menu;
                                *selected = 3; // Sessions
                            }
                        }
                        _ => {}
                    }
                }
                PauseView::CorpusSearch => {
                    // Check if results exist — controls whether W/X/G/Space act as navigation or text input
                    let has_results = if let Some(PopupContent::PauseMenu { ref search_results, .. }) = self.active_popup {
                        !search_results.is_empty()
                    } else { false };

                    match key {
                        KeyCode::Esc => {
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                *view = PauseView::Menu;
                                *selected = 4; // Corpus Search
                            }
                        }
                        // Arrow keys always navigate results
                        KeyCode::Up => {
                            if let Some(PopupContent::PauseMenu { ref mut search_selected, .. }) = self.active_popup {
                                if *search_selected > 0 { *search_selected -= 1; }
                            }
                        }
                        KeyCode::Down => {
                            if let Some(PopupContent::PauseMenu { ref mut search_selected, ref search_results, .. }) = self.active_popup {
                                if *search_selected + 1 < search_results.len() { *search_selected += 1; }
                            }
                        }
                        // W/X navigate when results showing, otherwise type into search input
                        KeyCode::Char('w') | KeyCode::Char('W') if has_results => {
                            if let Some(PopupContent::PauseMenu { ref mut search_selected, .. }) = self.active_popup {
                                if *search_selected > 0 { *search_selected -= 1; }
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') if has_results => {
                            if let Some(PopupContent::PauseMenu { ref mut search_selected, ref search_results, .. }) = self.active_popup {
                                if *search_selected + 1 < search_results.len() { *search_selected += 1; }
                            }
                        }
                        // Gather selected result into inventory
                        KeyCode::Char('g') | KeyCode::Char('G') if has_results => {
                            let gather = if let Some(PopupContent::PauseMenu { ref search_results, ref search_selected, .. }) = self.active_popup {
                                search_results.get(*search_selected).map(|r| (
                                    r.canonical_id.clone(),
                                    r.coord,
                                    r.name.clone(),
                                    r.district.clone(),
                                ))
                            } else { None };

                            if let Some((id, coord, name, district)) = gather {
                                let room = coord.unwrap_or(self.engine.current_room);
                                let turn = self.engine.turn;
                                let display = if name.is_empty() { id.clone() } else { name.clone() };
                                if self.inventory.gather(id.clone(), room, turn, name, "unknown".into(), district) {
                                    self.push_gather_toast(&display, "unknown");
                                } else {
                                    self.status_message = Some(format!("{} already gathered", display));
                                }
                            }
                        }
                        // Space: open selected result as LecternRead
                        KeyCode::Char(' ') if has_results => {
                            let search_data = if let Some(PopupContent::PauseMenu { ref search_results, ref search_selected, .. }) = self.active_popup {
                                Some((
                                    *search_selected,
                                    search_results.iter().map(|r| r.canonical_id.clone()).collect::<Vec<_>>(),
                                ))
                            } else { None };
                            if let Some((selected_idx, all_ids)) = search_data {
                                let saved = self.active_popup.clone();
                                let ptypes = vec![String::new(); all_ids.len()];
                                self.shelf_context = Some((all_ids, ptypes, selected_idx));
                                self.shelf_source_label = Some("search".into());
                                self.read_volume_at_index(selected_idx).await;
                                if matches!(&self.active_popup, Some(PopupContent::LecternRead { .. })) {
                                    self.saved_popup = saved;
                                }
                            }
                        }
                        // Text input — catches remaining characters when results aren't intercepting
                        KeyCode::Char(c) if !c.is_control() => {
                            if let Some(PopupContent::PauseMenu { ref mut search_input, .. }) = self.active_popup {
                                search_input.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(PopupContent::PauseMenu { ref mut search_input, .. }) = self.active_popup {
                                search_input.pop();
                            }
                        }
                        KeyCode::Enter => {
                            let query = if let Some(PopupContent::PauseMenu { ref search_input, .. }) = self.active_popup {
                                search_input.trim().to_string()
                            } else { return; };
                            if query.is_empty() { return; }

                            // Set loading state + clear volume cache from previous search
                            self.volume_cache.clear();
                            if let Some(PopupContent::PauseMenu { ref mut search_loading, ref mut search_results, ref mut search_selected, .. }) = self.active_popup {
                                *search_loading = true;
                                search_results.clear();
                                *search_selected = 0;
                            }

                            // Spawn background search — results drained in game loop
                            let bfs = self.bfs.clone();
                            self.search_pending = Some(tokio::spawn(async move {
                                let hits = bfs.search(&query, 20).await.unwrap_or_default();
                                hits.into_iter().map(|h| {
                                    render::popup::CorpusSearchHit {
                                        canonical_id: h.canonical_id,
                                        name: h.conventional_name.unwrap_or_default(),
                                        snippet: h.snippet.unwrap_or_default(),
                                        coord: h.coord,
                                        district: h.district.unwrap_or_default(),
                                        z_band: h.z_band,
                                    }
                                }).collect()
                            }));
                        }
                        _ => {}
                    }
                }
                PauseView::Articles => {
                    match key {
                        KeyCode::Esc => {
                            // If reading an article, go back to list; if on list, go to menu
                            let has_body = if let Some(PopupContent::PauseMenu { ref article_body, .. }) = self.active_popup {
                                !article_body.is_empty()
                            } else { false };

                            if has_body {
                                // Check if we're scrolled into the article — go back to list view
                                if let Some(PopupContent::PauseMenu { ref mut article_body, ref mut article_scroll, .. }) = self.active_popup {
                                    article_body.clear();
                                    *article_scroll = 0;
                                }
                            } else {
                                if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                    *view = PauseView::Menu;
                                    *selected = 5; // Articles
                                }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => {
                            if let Some(PopupContent::PauseMenu { ref article_body, ref mut article_selected, ref mut article_scroll, .. }) = self.active_popup {
                                if !article_body.is_empty() {
                                    // Scroll article up (toward top)
                                    *article_scroll = article_scroll.saturating_sub(3);
                                } else {
                                    // Navigate list up
                                    if *article_selected > 0 { *article_selected -= 1; }
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('x') | KeyCode::Char('X') => {
                            if let Some(PopupContent::PauseMenu { ref article_body, ref mut article_selected, ref article_list, ref mut article_scroll, .. }) = self.active_popup {
                                if !article_body.is_empty() {
                                    // Scroll article down (toward bottom)
                                    *article_scroll += 3;
                                } else {
                                    // Navigate list down
                                    if *article_selected + 1 < article_list.len() { *article_selected += 1; }
                                }
                            }
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            // Load selected article
                            let path = if let Some(PopupContent::PauseMenu { ref article_list, ref article_selected, .. }) = self.active_popup {
                                article_list.get(*article_selected).map(|a| {
                                    format!("{}/content/{}", articles_dir(), a.filename)
                                })
                            } else { None };

                            if let Some(path) = path {
                                if let Ok(text) = std::fs::read_to_string(&path) {
                                    if let Some(PopupContent::PauseMenu { ref mut article_body, ref mut article_scroll, .. }) = self.active_popup {
                                        *article_body = text;
                                        *article_scroll = 0;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                PauseView::RoomLookup => {
                    match key {
                        KeyCode::Esc => {
                            if let Some(PopupContent::PauseMenu { ref mut view, ref mut selected, .. }) = self.active_popup {
                                *view = PauseView::Menu;
                                *selected = 6; // Room Lookup
                            }
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() || c == ',' || c == '-' || c == ' ' => {
                            if let Some(PopupContent::PauseMenu { ref mut coord_input, ref mut lookup_result, .. }) = self.active_popup {
                                coord_input.push(c);
                                *lookup_result = None; // Clear previous result on new input
                            }
                        }
                        KeyCode::Backspace => {
                            if let Some(PopupContent::PauseMenu { ref mut coord_input, ref mut lookup_result, .. }) = self.active_popup {
                                coord_input.pop();
                                *lookup_result = None;
                            }
                        }
                        KeyCode::Enter => {
                            // Parse coords and do BFS lookup
                            let input = if let Some(PopupContent::PauseMenu { ref coord_input, .. }) = self.active_popup {
                                coord_input.clone()
                            } else { return; };

                            let result = self.lookup_room_coords(&input).await;

                            if let Some(PopupContent::PauseMenu { ref mut lookup_result, .. }) = self.active_popup {
                                *lookup_result = Some(result);
                            }
                        }
                        KeyCode::Char(' ') => {
                            // Navigate to looked-up room if result exists and valid
                            let nav_coord = if let Some(PopupContent::PauseMenu { ref lookup_result, .. }) = self.active_popup {
                                lookup_result.as_ref()
                                    .filter(|r| r.error.is_none())
                                    .map(|r| r.coord)
                            } else { None };

                            if let Some(coord) = nav_coord {
                                self.active_popup = None;
                                self.navigate_to_room(coord).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
            return;
        }

        // Dialog is non-blocking: Esc closes, all other keys fall through to game input
        if matches!(&self.active_popup, Some(PopupContent::Dialog { .. })) {
            if key == KeyCode::Esc {
                self.active_popup = None;
                return;
            }
            // Fall through — movement keys still work with dialog open
        } else if self.active_popup.is_some() {
            let popup_type = match &self.active_popup {
                Some(PopupContent::ShelfMenu { .. }) => "shelf_menu",
                Some(PopupContent::LecternRead { .. }) => "reading",
                Some(PopupContent::ArticleRead { .. }) => "article_read",
                _ => "other",
            };

            match popup_type {
                "shelf_menu" => {
                    match key {
                        KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                            if let Some(PopupContent::ShelfMenu { ref mut selected, .. }) = self.active_popup {
                                if *selected > 0 { *selected -= 1; }
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Down => {
                            if let Some(PopupContent::ShelfMenu { ref mut selected, ref entries, .. }) = self.active_popup {
                                if *selected + 1 < entries.len() { *selected += 1; }
                            }
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            let idx = if let Some(PopupContent::ShelfMenu { selected, .. }) = &self.active_popup {
                                Some(*selected)
                            } else { None };
                            if let Some(i) = idx {
                                let saved = self.active_popup.clone();
                                self.read_volume_at_index(i).await;
                                if matches!(&self.active_popup, Some(PopupContent::LecternRead { .. })) {
                                    self.saved_popup = saved;
                                }
                            }
                        }
                        KeyCode::Char('g') | KeyCode::Char('G') => {
                            // Gather selected volume to inventory
                            let gather_data = if let Some(PopupContent::ShelfMenu { ref entries, selected, .. }) = self.active_popup {
                                entries.get(selected).map(|(id, name, ptype)| (id.clone(), name.clone(), ptype.clone()))
                            } else { None };
                            if let Some((id, name, ptype)) = gather_data {
                                    let room = self.engine.current_room;
                                    let turn = self.engine.turn;
                                    let district = self.world.rooms.get(&room)
                                        .map(|r| r.info.district.clone())
                                        .unwrap_or_default();
                                    if self.inventory.gather(id.clone(), room, turn, name.clone(), ptype.clone(), district) {
                                        let fixture_coord = self.world.find_fixture_coord_for_volume(&room, &id);
                                        self.spawn_gather_vortex_player(fixture_coord, &name, &ptype);
                                    } else {
                                        self.status_message = Some("Already in your collection".into());
                                    }
                            }
                        }
                        KeyCode::Esc => {
                            self.active_popup = None;
                            self.shelf_context = None;
                            self.saved_popup = None;
                            self.status_message = None;
                        }
                        _ => {}
                    }
                }
                "reading" => {
                    match key {
                        // Flip to next/previous text on the shelf
                        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => {
                            if let Some((ref ids, _, idx)) = self.shelf_context {
                                if idx + 1 < ids.len() {
                                    self.read_volume_at_index(idx + 1).await;
                                }
                            }
                        }
                        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => {
                            if let Some((_, _, idx)) = self.shelf_context {
                                if idx > 0 {
                                    self.read_volume_at_index(idx - 1).await;
                                }
                            }
                        }
                        // Scroll within current text
                        KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                            if let Some(PopupContent::LecternRead { ref mut scroll_offset, .. }) = self.active_popup {
                                if *scroll_offset > 0 { *scroll_offset -= 1; }
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Down => {
                            if let Some(PopupContent::LecternRead { ref mut scroll_offset, ref sentences, .. }) = self.active_popup {
                                if *scroll_offset + 1 < sentences.len() { *scroll_offset += 1; }
                            }
                        }
                        // Esc: back to saved popup (shelf menu / pause menu) or close
                        KeyCode::Esc => {
                            if let Some(mut saved) = self.saved_popup.take() {
                                // Update cursor position in saved popup to match where user left off
                                if let (PopupContent::ShelfMenu { ref mut selected, .. }, Some((_, _, idx))) = (&mut saved, &self.shelf_context) {
                                    *selected = *idx;
                                }
                                if let PopupContent::PauseMenu { ref view, ref mut search_selected, ref mut inventory_selected, .. } = saved {
                                    if let Some((_, _, idx)) = &self.shelf_context {
                                        if matches!(view, render::popup::PauseView::CorpusSearch) {
                                            *search_selected = *idx;
                                        } else if matches!(view, render::popup::PauseView::Inventory) {
                                            *inventory_selected = *idx;
                                        }
                                    }
                                }
                                let keep_shelf = matches!(&saved, PopupContent::ShelfMenu { .. });
                                self.active_popup = Some(saved);
                                if !keep_shelf {
                                    self.shelf_context = None;
                                }
                                self.shelf_source_label = None;
                                self.status_message = None;
                            } else {
                                self.active_popup = None;
                                self.shelf_context = None;
                                self.shelf_source_label = None;
                                self.status_message = None;
                            }
                        }
                        // Gather current text to inventory
                        KeyCode::Char('g') | KeyCode::Char('G') => {
                            let gather_data = if let (
                                Some(PopupContent::LecternRead { ref title, .. }),
                                Some((ref ids, ref ptypes, idx)),
                            ) = (&self.active_popup, &self.shelf_context) {
                                ids.get(*idx).map(|id| (id.clone(), title.clone(), ptypes.get(*idx).cloned().unwrap_or_default()))
                            } else { None };
                            if let Some((id, name, ptype)) = gather_data {
                                let room = self.engine.current_room;
                                let turn = self.engine.turn;
                                let district = self.world.rooms.get(&room)
                                    .map(|r| r.info.district.clone())
                                    .unwrap_or_default();
                                if self.inventory.gather(id.clone(), room, turn, name.clone(), ptype.clone(), district) {
                                    let fixture_coord = self.world.find_fixture_coord_for_volume(&room, &id);
                                    self.spawn_gather_vortex_player(fixture_coord, &name, &ptype);
                                } else {
                                    self.status_message = Some("Already in your collection".into());
                                }
                            }
                        }
                        KeyCode::Char(' ') => {
                            self.active_popup = None;
                            self.shelf_context = None;
                            self.saved_popup = None;
                            self.status_message = None;
                        }
                        _ => {}
                    }
                }
                "article_read" => {
                    match key {
                        KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Up => {
                            if let Some(PopupContent::ArticleRead { ref mut scroll_offset, .. }) = self.active_popup {
                                *scroll_offset = scroll_offset.saturating_sub(3);
                            }
                        }
                        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Down => {
                            if let Some(PopupContent::ArticleRead { ref mut scroll_offset, .. }) = self.active_popup {
                                *scroll_offset += 3;
                            }
                        }
                        KeyCode::Esc => {
                            self.active_popup = None;
                        }
                        _ => {}
                    }
                }
                _ => {
                    // Generic popup: Space or Esc to close
                    match key {
                        KeyCode::Char(' ') | KeyCode::Esc => {
                            self.active_popup = None;
                            self.shelf_context = None;
                            self.status_message = None;
                        }
                        _ => {}
                    }
                }
            }
            return;
        }

        // HUD toggle (before engine dispatch)
        if key == KeyCode::Tab {
            self.hud_mode = self.hud_mode.next();
            return;
        }

        // Esc with no popup open → pause menu
        if key == KeyCode::Esc {
            let inv_items: Vec<(String, [i32; 3], String, String, String)> = self.inventory.volumes
                .iter()
                .map(|v| (v.canonical_id.clone(), v.room_coord, v.name.clone(), v.physical_type.clone(), v.district.clone()))
                .collect();
            self.active_popup = Some(PopupContent::PauseMenu {
                selected: 0,
                view: render::popup::PauseView::Menu,
                coord_input: String::new(),
                lookup_result: None,
                inventory_items: inv_items,
                inventory_selected: 0,
                session_list: Vec::new(),
                session_selected: 0,
                sessions_loading: false,
                map_center: self.engine.current_room,
                search_input: String::new(),
                search_results: Vec::new(),
                search_selected: 0,
                search_loading: false,
                article_list: Vec::new(),
                article_selected: 0,
                article_body: String::new(),
                article_scroll: 0,
            });
            return;
        }

        if let Some(action) = map_key_to_action(key) {
            let result = self.engine.process_action(action, &self.world);
            match result {
                ActionResult::Ok => {
                    // Show contextual hint if on a shaft tile
                    let player_tile = self.world.tiles.get(&self.engine.player.coord());
                    if let Some(wt) = player_tile {
                        if wt.tile.is_shaft() {
                            self.status_message = Some("↑/↓ or Space to traverse shaft".into());
                        } else {
                            self.status_message = None;
                        }
                    } else {
                        self.status_message = None;
                    }
                    // Incrementally preload rooms in proximity
                    self.spawn_preload_proximity();
                }
                ActionResult::Quit => {
                    self.should_quit = true;
                }
                ActionResult::Blocked(reason) => {
                    self.status_message = Some(reason.to_string());
                }
                ActionResult::EnteredRoom(coord) => {
                    // Track breadcrumb trail
                    if self.breadcrumb_trail.back() != Some(&coord) {
                        self.breadcrumb_trail.push_back(coord);
                        if self.breadcrumb_trail.len() > 8 {
                            self.breadcrumb_trail.pop_front();
                        }
                    }

                    let old_z = self.engine.current_room[2];
                    self.engine.current_room = coord;

                    // Ensure destination room is loaded (single await — fast if cached)
                    if !self.world.rooms.contains_key(&coord) {
                        if let Ok(meta) = self.bfs.get_room(coord[0], coord[1], coord[2]).await {
                            self.world.place_room(&meta);
                        }
                    }
                    self.world.touch_room(coord);

                    // Background-preload surrounding neighborhood
                    self.spawn_preload_proximity();

                    if let Some(room) = self.world.rooms.get(&coord) {
                        // Teleport player to target room on z-change
                        if coord[2] != old_z {
                            self.engine.player.pos.coord = room.center;
                            self.engine.teleport_librarian_to_player(&self.world);
                            self.engine.teleport_scribe_to_librarian(&self.world);
                        }

                        self.current_palette = Palette::from_room(
                            room.info.z, &room.info.district
                        );

                        // Set atmospheric entry text (persists ~4 seconds)
                        let atmo = atmospheric_text(
                            &room.info.room_type,
                            &room.info.district,
                            room.info.volume_count,
                            room.info.z,
                        );
                        self.atmosphere_text = Some(atmo.to_string());
                        self.atmosphere_expires = Some(Instant::now() + std::time::Duration::from_secs(4));

                        // Cache composition summary
                        let comp = RoomComposition::from_volumes_typed(&room.info.volumes_typed);
                        self.composition_cache = composition_summary(&comp);

                        // Initialize particles for new room
                        if !self.room_particles.contains_key(&self.engine.current_room) {
                            let rp = init_room_particles(
                                self.engine.current_room,
                                &self.world.tiles,
                                room.info.z,
                                &room.info.district,
                                room.info.volume_count as usize,
                            );
                            self.room_particles.insert(self.engine.current_room, rp);
                        }

                        self.status_message = None;

                        // Dump tile grid for agent pathfinding
                        game::agent_state::write_tile_grid(
                            &self.world,
                            self.engine.player.coord(),
                            self.engine.current_room,
                        );
                    }
                }
                ActionResult::PortalEnter => {
                    self.handle_portal_enter().await;
                }
                ActionResult::InspectShelf(tile_coord) => {
                    self.show_shelf_popup(tile_coord).await;
                }
                ActionResult::ReadLectern => {
                    self.show_lectern_popup().await;
                }
                ActionResult::ReadArticle => {
                    self.show_article_display();
                }
                ActionResult::LibrarianInteract => {
                    // Restore previous chat state or start fresh
                    let (msgs, buf, cursor, scroll) = if let Some(cs) = self.chat_state.take() {
                        (cs.messages, cs.input_buffer, cs.input_cursor, cs.scroll_offset)
                    } else {
                        (Vec::new(), String::new(), 0, 0)
                    };

                    self.active_popup = Some(PopupContent::Chat {
                        messages: msgs,
                        input_buffer: buf,
                        input_cursor: cursor,
                        streaming_partial: String::new(),
                        streaming_is_thinking: false,
                        status: ChatStatus::Idle,
                        scroll_offset: scroll,
                        status_text: None,
                    });
                }
                ActionResult::ScribeInteract => {
                    self.open_scribe_chat();
                }
            }
        }
    }

    /// Name of the active popup for agent state reporting.
    fn popup_name(&self) -> Option<&'static str> {
        self.active_popup.as_ref().map(|p| match p {
            PopupContent::ShelfInspect { .. } => "ShelfInspect",
            PopupContent::ShelfMenu { .. } => "ShelfMenu",
            PopupContent::LecternRead { .. } => "LecternRead",
            PopupContent::Dialog { .. } => "Dialog",
            PopupContent::ScribeChat { .. } => "ScribeChat",
            PopupContent::PauseMenu { .. } => "PauseMenu",
            PopupContent::Chat { .. } => "Chat",
            PopupContent::ArticleRead { .. } => "ArticleRead",
        })
    }

    async fn show_shelf_popup(&mut self, tile_coord: hex2d_stacks::Coordinate<i32>) {
        let room_coord = self.engine.current_room;
        let world_tile = self.world.tiles.get(&tile_coord);
        let tile = world_tile.map(|wt| wt.tile);
        let fixture_id = world_tile.and_then(|wt| wt.fixture_id);

        // Build typed volume map for this room
        let typed_map: std::collections::HashMap<String, String> = self.world.rooms.get(&room_coord)
            .map(|r| r.info.volumes_typed.iter()
                .map(|vt| (vt.id.clone(), vt.physical_type.clone()))
                .collect())
            .unwrap_or_default();

        // --- Path 1: Fixture with ID → direct shelf lookup (no ambiguity) ---
        if let Some(fid) = fixture_id {
            let fixture = self.world.fixtures.get(&room_coord)
                .and_then(|flist| flist.iter().find(|f| f.id == fid));

            if let Some(fix) = fixture {
                let shelf_idx = fix.shelf_index;
                let label = fix.label.clone();
                let shelves = self.world.shelves.get(&room_coord);
                let volumes = shelves.and_then(|s| s.get(shelf_idx)).filter(|s| !s.is_empty());

                match volumes {
                    Some(vols) => {
                        let mut entries: Vec<(String, String, String)> = Vec::new();
                        let mut ids: Vec<String> = Vec::new();
                        let mut ptypes: Vec<String> = Vec::new();
                        for id in vols {
                            let name = match self.bfs.get_volume(id).await {
                                Ok(vol) => vol.conventional_name.unwrap_or_else(|| id.clone()),
                                Err(_) => id.clone(),
                            };
                            let ptype = typed_map.get(id).cloned().unwrap_or_default();
                            entries.push((id.clone(), name, ptype.clone()));
                            ids.push(id.clone());
                            ptypes.push(ptype);
                        }

                        self.shelf_context = Some((ids, ptypes, 0));
                        let title = format!("{} \u{2014} {} texts", label, entries.len());
                        self.active_popup = Some(PopupContent::ShelfMenu {
                            title,
                            entries,
                            selected: 0,
                        });
                        self.status_message = Some("W/X select | Space read | Esc close".into());
                    }
                    None => {
                        self.status_message = Some(format!("{} \u{2014} empty", label));
                    }
                }
                return;
            }
        }

        // --- Path 2: Generic Shelf tile (no fixture_id) → face-based shelf lookup ---
        // If a fixture on this face already owns the shelf data, skip the duplicate
        // and point the player to the fixture or the lectern.
        let room = self.world.rooms.get(&room_coord);
        let shelves = self.world.shelves.get(&room_coord);
        let num_shelves = shelves.map(|s| s.len()).unwrap_or(0);
        let tile_face = world_tile.and_then(|wt| wt.face);

        let face_label = match tile_face {
            Some(game::HexFace::E)  => "East",
            Some(game::HexFace::NE) => "Northeast",
            Some(game::HexFace::NW) => "Northwest",
            Some(game::HexFace::W)  => "West",
            Some(game::HexFace::SW) => "Southwest",
            Some(game::HexFace::SE) => "Southeast",
            None => "Wall",
        };

        let shelf_idx: Option<usize> = if num_shelves > 0 {
            if let (Some(room), Some(face)) = (room, tile_face) {
                let face_order = [
                    game::HexFace::E, game::HexFace::NE, game::HexFace::NW,
                    game::HexFace::W, game::HexFace::SW, game::HexFace::SE,
                ];
                let non_door_faces: Vec<game::HexFace> = face_order.iter()
                    .enumerate()
                    .filter(|(i, _)| room.layout.walls[*i] != game::world::WallState::FreeDoor)
                    .map(|(_, f)| *f)
                    .collect();
                let face_pos = non_door_faces.iter().position(|f| *f == face).unwrap_or(0);
                if face_pos < num_shelves { Some(face_pos) } else { None }
            } else { Some(0) }
        } else { None };

        // If a fixture already claims this shelf index, don't duplicate —
        // point the player to the fixture or the lectern.
        if let Some(idx) = shelf_idx {
            if let Some(flist) = self.world.fixtures.get(&room_coord) {
                if let Some(fix) = flist.iter().find(|f| f.shelf_index == idx) {
                    let vol_count = self.world.shelves.get(&room_coord)
                        .and_then(|s| s.get(fix.shelf_index))
                        .map(|v| v.len())
                        .unwrap_or(0);
                    let location = if fix.tile_type.is_passable() {
                        "on the floor"
                    } else {
                        "on the wall"
                    };
                    self.status_message = Some(format!(
                        "{} ({} texts) \u{2014} {} \u{2014} use lectern for all texts",
                        fix.label, vol_count, location
                    ));
                    return;
                }
            }
        }

        let shelf_volumes = shelf_idx
            .and_then(|idx| shelves.and_then(|s| s.get(idx)))
            .filter(|s| !s.is_empty());

        match shelf_volumes {
            Some(volumes) => {
                let mut entries: Vec<(String, String, String)> = Vec::new();
                let mut ids: Vec<String> = Vec::new();
                let mut ptypes: Vec<String> = Vec::new();
                for id in volumes.iter() {
                    let name = match self.bfs.get_volume(id).await {
                        Ok(vol) => vol.conventional_name.unwrap_or_else(|| id.clone()),
                        Err(_) => id.clone(),
                    };
                    let ptype = typed_map.get(id).cloned().unwrap_or_default();
                    entries.push((id.clone(), name, ptype.clone()));
                    ids.push(id.clone());
                    ptypes.push(ptype);
                }

                self.shelf_context = Some((ids, ptypes, 0));

                let tile_label = match tile {
                    Some(game::Tile::PapyrusTablet) => "Papyrus Shelf",
                    Some(game::Tile::WallInscription) => "Wall Inscriptions",
                    Some(game::Tile::FalseDoor) => "False Door",
                    Some(game::Tile::TombAlcove) => "Tomb Alcove",
                    Some(game::Tile::Shelf) => "Shelf",
                    _ => "Wall Section",
                };
                let title = format!("{} \u{2014} {} Wall \u{2014} {} texts",
                    tile_label, face_label, entries.len());
                self.active_popup = Some(PopupContent::ShelfMenu {
                    title,
                    entries,
                    selected: 0,
                });
                self.status_message = Some("W/X select | Space read | Esc close".into());
            }
            _ => self.status_message = Some(format!("{} wall \u{2014} bare stone, no texts", face_label)),
        }
    }

    /// Open a specific volume for reading, with physical type and position info
    async fn read_volume_at_index(&mut self, index: usize) {
        let (ids, ptypes, _) = match &self.shelf_context {
            Some(ctx) => ctx.clone(),
            None => return,
        };
        let total = ids.len();
        if index >= total { return; }

        let canonical_id = &ids[index];
        let physical_type = &ptypes[index];

        // Check volume cache first (search results), then BFS fetch
        let vol = if let Some(cached) = self.volume_cache.get(canonical_id) {
            cached.clone()
        } else {
            match self.bfs.get_volume(canonical_id).await {
                Ok(vol) => {
                    self.volume_cache.insert(canonical_id.clone(), vol.clone());
                    vol
                }
                Err(e) => {
                    self.status_message = Some(format!("Cannot read: {}", e));
                    return;
                }
            }
        };

        let title = vol.conventional_name.clone()
            .unwrap_or_else(|| canonical_id.to_string());

        let mut sentences: Vec<(Option<String>, Option<String>)> = Vec::new();

        // Header: title (gold)
        sentences.push((Some(title.clone()), None));
        // Source line: type + canonical ID (gray)
        let type_label = if physical_type.is_empty() { "text".to_string() } else { physical_type.clone() };
        // Count how many of this type exist on this shelf
        let same_type_count = ptypes.iter().filter(|t| *t == physical_type).count();
        let same_type_idx = ptypes.iter().take(index + 1).filter(|t| *t == physical_type).count();
        let type_display = capitalize_type(&type_label);
        sentences.push((Some(format!("{}: {}/{} \u{2014} {}", type_display, same_type_idx, same_type_count, canonical_id)), None));
        // Position label — "search" when browsing search results, "collection" otherwise
        let source_label = self.shelf_source_label.as_deref().unwrap_or("collection");
        sentences.push((Some(format!("Text {} of {} in this {}", index + 1, total, source_label)), None));
        sentences.push((None, None)); // blank line

        // Sentences: transliteration (gray) + translation (white)
        for s in &vol.sentences {
            let trans = s.translation.get("en")
                .or_else(|| s.translation.get("de"))
                .filter(|t| !t.starts_with("[ERROR"))
                .cloned();
            sentences.push((s.transliteration.clone(), trans));
        }

        if vol.sentences.is_empty() {
            sentences.push((None, Some("(no sentence data available)".to_string())));
        }

        // Update shelf context with current index
        self.shelf_context = Some((ids, ptypes, index));

        self.active_popup = Some(PopupContent::LecternRead {
            title,
            sentences,
            scroll_offset: 0,
        });
        self.status_message = Some("\u{2190}/\u{2192} flip texts | Esc back".into());
    }

    /// Open article reader popup for the ArticleDisplay tile the player is standing on.
    fn show_article_display(&mut self) {
        let player_coord = self.engine.player.coord();
        let filename = match self.world.article_tiles.get(&player_coord) {
            Some(f) => f.clone(),
            None => { self.status_message = Some("No article here".into()); return; }
        };
        let path = format!("{}/content/{}", articles_dir(), filename);
        let body = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => { self.status_message = Some(format!("Cannot read article: {}", e)); return; }
        };
        let title = body.lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches('#').trim().trim_matches('"').to_string())
            .unwrap_or_else(|| filename.trim_end_matches(".md").replace('-', " "));
        self.active_popup = Some(PopupContent::ArticleRead {
            title,
            body,
            scroll_offset: 0,
        });
    }

    async fn show_lectern_popup(&mut self) {
        let room_coord = self.engine.current_room;
        let room = self.world.rooms.get(&room_coord);

        // Build room identity header from cluster data
        let room_name = if let Some(r) = room {
            room_name_from_cluster(
                &r.info.cluster_id,
                &r.info.cluster_signature,
                &r.info.room_type,
            )
        } else {
            "Unknown Room".to_string()
        };

        // Collect all corpus IDs in this room
        let all_volumes: Vec<String> = self.world.shelves.get(&room_coord)
            .map(|shelves| shelves.iter().flatten().cloned().collect())
            .unwrap_or_default();

        if all_volumes.is_empty() {
            self.status_message = Some("Lectern is empty — no corpus data in this room".into());
            return;
        }

        // Build typed volume map for fixture type display
        let typed_map: std::collections::HashMap<String, String> = room
            .map(|r| r.info.volumes_typed.iter()
                .map(|vt| (vt.id.clone(), vt.physical_type.clone()))
                .collect())
            .unwrap_or_default();

        // Build reverse map: volume_id → fixture label (e.g. "Papyrus Shelf", "Stele")
        // Fixtures reference shelves by index; shelves contain volume IDs.
        let fixture_label_map: std::collections::HashMap<String, String> = {
            let mut map = std::collections::HashMap::new();
            if let (Some(fixtures), Some(shelves)) = (
                self.world.fixtures.get(&room_coord),
                self.world.shelves.get(&room_coord),
            ) {
                for fix in fixtures {
                    if let Some(vol_ids) = shelves.get(fix.shelf_index) {
                        for vid in vol_ids {
                            map.insert(vid.clone(), fix.label.clone());
                        }
                    }
                }
            }
            map
        };

        // Build ShelfMenu entries: (canonical_id, display_name, physical_type)
        let mut entries: Vec<(String, String, String)> = Vec::new();
        let mut ids: Vec<String> = Vec::new();
        let mut ptypes: Vec<String> = Vec::new();
        for vol_id in &all_volumes {
            let ptype = typed_map.get(vol_id).cloned().unwrap_or_default();
            let base_name = match self.bfs.get_volume(vol_id).await {
                Ok(vol) => vol.conventional_name.unwrap_or_else(|| vol_id.clone()),
                Err(_) => vol_id.clone(),
            };
            // Annotate with fixture location when available
            let name = if let Some(label) = fixture_label_map.get(vol_id) {
                format!("{} \u{2014} {}", base_name, label)
            } else {
                base_name
            };
            entries.push((vol_id.clone(), name, ptype.clone()));
            ids.push(vol_id.clone());
            ptypes.push(ptype);
        }

        // Store shelf context so read_volume_at_index works (reuses shelf reading flow)
        self.shelf_context = Some((ids, ptypes, 0));

        let district_display = room.map(|r| display_district(&r.info.district)).unwrap_or("?");
        let era = room.map(|r| era_name_from_z(r.info.z)).unwrap_or("?");
        let title = format!(
            "{} \u{2014} {} | {} \u{2014} {} texts",
            room_name, district_display, era, entries.len()
        );

        self.active_popup = Some(PopupContent::ShelfMenu {
            title,
            entries,
            selected: 0,
        });
        self.status_message = Some("W/X select | Space read | G gather | Esc close".into());
    }

    /// Collect coords that need fetching (sync, instant) and spawn a background
    /// task to batch-fetch them. Results are drained in `drain_preload_results()`.
    fn spawn_preload_proximity(&mut self) {
        const PRELOAD_DEPTH: usize = 7;
        const MAX_FETCHES: usize = 24;

        let mut queue: VecDeque<([i32; 3], usize)> = VecDeque::new();
        let mut visited: HashSet<[i32; 3]> = HashSet::new();
        let mut to_fetch: Vec<[i32; 3]> = Vec::new();

        let start = self.engine.current_room;
        queue.push_back((start, 0));
        visited.insert(start);

        // Walk loaded rooms, collect unloaded coords
        while let Some((coord, d)) = queue.pop_front() {
            if d >= PRELOAD_DEPTH { continue; }

            if let Some(room) = self.world.rooms.get(&coord) {
                for to_coord in room_info_neighbor_coords(&room.info) {
                    if !visited.contains(&to_coord) {
                        visited.insert(to_coord);
                        queue.push_back((to_coord, d + 1));
                    }
                }
            } else if !self.bfs_miss_cache.contains(&coord) {
                to_fetch.push(coord);
                if to_fetch.len() >= MAX_FETCHES { break; }
            }
        }

        if to_fetch.is_empty() {
            return;
        }

        // Spawn background fetch task
        let bfs = self.bfs.clone();
        let tx = self.preload_tx.clone();
        self.preload_in_flight = true;
        tokio::spawn(async move {
            match bfs.get_rooms(&to_fetch).await {
                Ok(results) => {
                    for (coord, result) in to_fetch.iter().zip(results.into_iter()) {
                        let msg = match result {
                            Some(meta) => PreloadResult::Room(meta),
                            None => PreloadResult::Miss(*coord),
                        };
                        let _ = tx.send(msg).await;
                    }
                }
                Err(_) => {} // network failure — skip silently
            }
            let _ = tx.send(PreloadResult::Done).await;
        });
    }

    /// Drain game commands from stacks-game.py socket (called each game tick).
    async fn drain_command_requests(&mut self) {
        while let Ok(req) = self.command_rx.try_recv() {
            let ack = self.execute_game_command_with_ack(req.command).await;
            let _ = req.ack_tx.send(ack);
        }
    }

    /// Drain completed preload results into the world (called each game tick).
    fn drain_preload_results(&mut self) {
        let mut missing: HashSet<[i32; 3]> = HashSet::new();
        let mut got_rooms = false;

        while let Ok(result) = self.preload_rx.try_recv() {
            match result {
                PreloadResult::Room(meta) => {
                    self.world.place_room(&meta);
                    got_rooms = true;
                }
                PreloadResult::Miss(coord) => {
                    self.bfs_miss_cache.insert(coord);
                    missing.insert(coord);
                }
                PreloadResult::Done => {
                    self.preload_in_flight = false;
                }
            }
        }

        if !missing.is_empty() {
            self.world.seal_dangling_doors(&missing);
        }

        // Evict rooms beyond the cap
        if got_rooms {
            self.run_eviction();
        }

        // If we loaded new rooms and no task in flight, kick off another
        // round to expand through newly-loaded rooms' edges.
        if got_rooms && !self.preload_in_flight {
            self.spawn_preload_proximity();
        }
    }

    /// Evict rooms beyond the LRU cap, protecting current room + neighbors.
    fn run_eviction(&mut self) {
        let mut protected = HashSet::new();
        let cur = self.engine.current_room;
        protected.insert(cur);
        if let Some(room) = self.world.rooms.get(&cur) {
            for edge in &room.info.door_edges {
                protected.insert(edge.to_coord);
            }
            for edge in &room.info.shaft_edges {
                protected.insert(edge.to_coord);
            }
        }
        // Protect portal origin/destination rooms from eviction
        if let Some(ref portal) = self.world.active_portal {
            protected.insert(portal.origin_room);
            protected.insert(portal.dest_room);
        }
        let evicted = self.world.evict_distant(&protected);
        for coord in &evicted {
            self.room_particles.remove(coord);
        }
    }

    /// Push a gather toast notification (stacks at top-left, expires after 8s).
    fn push_gather_toast(&mut self, name: &str, physical_type: &str) {
        let (glyph, color) = render::hud::glyph_for_physical_type(physical_type);
        let text = format!("Gathered {}", name);
        self.gather_toasts.push((text, glyph, color, Instant::now() + std::time::Duration::from_secs(8)));
        // Cap at 8 visible toasts
        if self.gather_toasts.len() > 8 {
            self.gather_toasts.remove(0);
        }
    }

    /// Spawn a player-initiated gather vortex animation.
    /// Toast fires on animation completion instead of immediately.
    fn spawn_gather_vortex_player(
        &mut self,
        fixture_coord: Option<hex2d_stacks::Coordinate<i32>>,
        name: &str,
        physical_type: &str,
    ) {
        let player_pos = self.engine.player.coord();
        let room = self.engine.current_room;
        // Use fixture coord if known, otherwise pick a nearby fixture or offset from player
        let source = fixture_coord.unwrap_or_else(|| {
            player_pos + hex2d_stacks::Coordinate::from(self.engine.player.facing())
                .scale(2)
        });
        self.gather_vortices.push(GatherVortex::new_player(
            source, player_pos, room,
            name.to_string(), physical_type.to_string(), self.tick,
        ));
    }

    /// Spawn a Hermes-initiated gather vortex animation (includes orbit phase).
    /// Accepts an optional stagger delay (in ticks) for batch gathers.
    fn spawn_gather_vortex_hermes(
        &mut self,
        volume_id: &str,
        name: &str,
        physical_type: &str,
        stagger: u64,
    ) {
        let player_pos = self.engine.player.coord();
        let room = self.engine.current_room;
        let source = self.world.find_fixture_coord_for_volume(&room, volume_id)
            .unwrap_or_else(|| {
                // Fallback: offset from player in a direction based on stagger index
                let dirs = [
                    hex2d_stacks::Direction::XY,
                    hex2d_stacks::Direction::XZ,
                    hex2d_stacks::Direction::YZ,
                    hex2d_stacks::Direction::YX,
                    hex2d_stacks::Direction::ZX,
                    hex2d_stacks::Direction::ZY,
                ];
                let dir = dirs[(stagger / 12) as usize % 6];
                player_pos + hex2d_stacks::Coordinate::from(dir).scale(3)
            });
        self.gather_vortices.push(GatherVortex::new_hermes(
            source, player_pos, room,
            name.to_string(), physical_type.to_string(),
            self.tick + stagger,
        ));
    }

    async fn handle_hermes_event(&mut self, event: HermesEvent) {
        match event {
            HermesEvent::SessionInfo { model, context_max } => {
                // Shorten model name for display (strip .gguf, path prefix)
                let short = model
                    .rsplit('/')
                    .next()
                    .unwrap_or(&model)
                    .trim_end_matches(".gguf")
                    .to_string();
                self.hermes_model = short;
                if context_max > 0 {
                    self.token_budget.context_window = context_max;
                }
                // Detect cloud vs local from model name or base_url
                // (default is "local", set at init; could be overridden by config)
            }
            HermesEvent::MessageStart => {
                self.chat_reasoning_buf.clear();
                if let Some(PopupContent::Chat { ref mut status, ref mut status_text, .. }) = self.active_popup {
                    *status = ChatStatus::Streaming;
                    *status_text = Some("generating response...".to_string());
                }
                // Mirror to sidebar state
                self.sidebar_status = ChatStatus::Streaming;
                self.sidebar_status_text = Some("generating response...".to_string());
            }
            HermesEvent::Delta { text } => {
                // Transition: if we were streaming reasoning, commit it before content
                if !self.chat_reasoning_buf.is_empty() {
                    self.chat_reasoning_buf.clear();
                    self.commit_streaming_partial();
                }
                if let Some(PopupContent::Chat { ref mut streaming_partial, ref mut status, ref mut scroll_offset, ref mut status_text, .. }) = self.active_popup {
                    streaming_partial.push_str(&text);
                    *status = ChatStatus::Streaming;
                    *status_text = None;
                    *scroll_offset = 0;
                }
                // Mirror to sidebar
                self.sidebar_streaming.push_str(&text);
                self.sidebar_status = ChatStatus::Streaming;
                self.sidebar_status_text = None;
            }
            HermesEvent::Complete { text, context_used, context_max, .. } => {
                // Extract commands and display text while borrowing popup
                let commands = {
                    // Use sidebar streaming if popup is closed
                    let final_text_from_sidebar = if self.active_popup.is_none() && !self.sidebar_streaming.is_empty() {
                        Some(std::mem::take(&mut self.sidebar_streaming))
                    } else {
                        None
                    };

                    if let Some(PopupContent::Chat {
                        ref mut messages,
                        ref mut streaming_partial,
                        ref mut status,
                        ref mut status_text,
                        ..
                    }) = self.active_popup {
                        let final_text = if streaming_partial.is_empty() {
                            text
                        } else {
                            std::mem::take(streaming_partial)
                        };

                        let cmds = game::hermes::parse_game_commands(&final_text);
                        let display_text = game::hermes::strip_commands(&final_text);

                        if !display_text.trim().is_empty() {
                            messages.push(ChatMessage::new(ChatRole::Librarian, display_text));
                        }
                        *status = ChatStatus::Idle;
                        *status_text = None;

                        if let Some(ref mut cs) = self.chat_state {
                            cs.messages = messages.clone();
                        }
                        cmds
                    } else if let Some(final_text) = final_text_from_sidebar {
                        // Popup closed (sidebar mode) — process into chat_state
                        let resolved = if final_text.is_empty() { text } else { final_text };
                        let cmds = game::hermes::parse_game_commands(&resolved);
                        let display_text = game::hermes::strip_commands(&resolved);
                        if !display_text.trim().is_empty() {
                            if let Some(ref mut cs) = self.chat_state {
                                cs.messages.push(ChatMessage::new(ChatRole::Librarian, display_text));
                            }
                        }
                        cmds
                    } else {
                        // Popup closed, no streaming — still process the text
                        let cmds = game::hermes::parse_game_commands(&text);
                        let display_text = game::hermes::strip_commands(&text);
                        if !display_text.trim().is_empty() {
                            if let Some(ref mut cs) = self.chat_state {
                                cs.messages.push(ChatMessage::new(ChatRole::Librarian, display_text));
                            }
                        }
                        cmds
                    }
                };
                // Clear sidebar streaming state
                self.sidebar_streaming.clear();
                self.sidebar_status = ChatStatus::Idle;
                self.sidebar_status_text = None;

                // Execute game commands (popup borrow released)
                for cmd in commands {
                    self.execute_game_command(cmd).await;
                }

                // Update token budget + trigger compression
                self.token_budget.update(context_used, context_max);
                if self.token_budget.should_new_session() {
                    // Context too full even after compression — start fresh
                    if let Some(ref mut bridge) = self.hermes_bridge {
                        bridge.close().await;
                    }
                    self.respawn_bridge().await;
                    if let Some(PopupContent::Chat { ref mut messages, .. }) = self.active_popup {
                        messages.push(ChatMessage::new(ChatRole::System, "The Librarian pauses to collect their thoughts.".to_string()));
                    }
                    self.token_budget = TokenBudget::new();
                } else if self.token_budget.should_compress() {
                    if let Some(ref mut bridge) = self.hermes_bridge {
                        let _ = bridge.compress().await;
                    }
                }
            }
            HermesEvent::StatusUpdate { text } => {
                // Status line only — don't clutter chat log
                self.sidebar_status_text = Some(text.clone());
                if let Some(PopupContent::Chat { ref mut status_text, .. }) = self.active_popup {
                    *status_text = Some(text);
                }
            }
            HermesEvent::ToolStart { name: _, context } => {
                // Commit any accumulated streaming text before tool transition
                self.chat_reasoning_buf.clear();
                self.commit_streaming_partial();

                // Transient status only — show command context, no tool name prefix
                let msg = if context.is_empty() {
                    "executing...".to_string()
                } else if context.len() > 80 {
                    format!("{}...", &context[..context.floor_char_boundary(77)])
                } else {
                    context
                };
                self.sidebar_status_text = Some(msg.clone());
                if let Some(PopupContent::Chat { ref mut status_text, .. }) = self.active_popup {
                    *status_text = Some(msg);
                }
            }
            HermesEvent::ToolComplete { .. } => {
                // Transient — clear status, do NOT log to chat history
                self.sidebar_status_text = None;
                if let Some(PopupContent::Chat { ref mut status_text, .. }) = self.active_popup {
                    *status_text = None;
                }
            }
            HermesEvent::ToolProgress { name: _, preview } => {
                let msg = if preview.len() > 80 { format!("{}...", &preview[..preview.floor_char_boundary(77)]) } else { preview };
                self.sidebar_status_text = Some(msg.clone());
                if let Some(PopupContent::Chat { ref mut status_text, .. }) = self.active_popup {
                    *status_text = Some(msg);
                }
            }
            HermesEvent::ThinkingDelta { text } => {
                // Stream reasoning into chat body as persistent growing text
                self.chat_reasoning_buf.push_str(&text);
                if let Some(PopupContent::Chat { ref mut streaming_partial, ref mut streaming_is_thinking, ref mut status, ref mut scroll_offset, .. }) = self.active_popup {
                    streaming_partial.push_str(&text);
                    *streaming_is_thinking = true;
                    *status = ChatStatus::Streaming;
                    *scroll_offset = 0;
                }
                // Mirror to sidebar
                self.sidebar_streaming.push_str(&text);
                self.sidebar_status = ChatStatus::Streaming;
            }
            HermesEvent::Error { ref message } if message == "Librarian gateway disconnected" => {
                // Auto-recovery: attempt respawn in background
                if let Some(PopupContent::Chat { ref mut messages, ref mut status, .. }) = self.active_popup {
                    messages.push(ChatMessage::new(ChatRole::System, "Connection lost. Reconnecting...".to_string()));
                    *status = ChatStatus::WaitingForAgent;
                }
                self.respawn_bridge().await;
            }
            HermesEvent::Error { message } => {
                if let Some(PopupContent::Chat { ref mut messages, ref mut status, .. }) = self.active_popup {
                    messages.push(ChatMessage::new(ChatRole::System, message));
                    *status = ChatStatus::Idle;
                }
            }
            _ => {}
        }
    }

    /// Execute a game command, returning an ack for the command server.
    async fn execute_game_command_with_ack(&mut self, cmd: game::hermes::GameCommand) -> game::CommandAck {
        match cmd {
            game::hermes::GameCommand::Navigate { coord } => {
                // Load target room if not already loaded
                if !self.world.rooms.contains_key(&coord) {
                    if let Ok(meta) = self.bfs.get_room(coord[0], coord[1], coord[2]).await {
                        self.world.place_room(&meta);
                    } else {
                        let msg = format!("Cannot reach room [{},{},{}]", coord[0], coord[1], coord[2]);
                        self.chat_log_system(&msg);
                        return game::CommandAck::fail(msg);
                    }
                }

                let dest_center = self.world.rooms.get(&coord).map(|r| r.center);
                if let Some(dest_center) = dest_center {
                    // Close chat UI (save state for later restoration)
                    self.save_chat_state();
                    self.active_popup = None;

                    // Spawn portal pair near the player
                    let player_pos = self.engine.player.coord();
                    let player_facing = self.engine.player.facing();
                    let origin_room = self.engine.current_room;
                    self.world.spawn_portal_pair(
                        player_pos, player_facing,
                        origin_room, coord, dest_center,
                    );

                    // Toast notification (same system as gather toasts)
                    self.gather_toasts.push((
                        "A portal shimmers open...".to_string(),
                        "▐▌", // portal glyph
                        ratatui::style::Color::Rgb(180, 60, 200),
                        Instant::now() + std::time::Duration::from_secs(5),
                    ));
                    self.status_message = None;
                    game::CommandAck::ok()
                } else {
                    let msg = format!("Room not found after load [{},{},{}]", coord[0], coord[1], coord[2]);
                    self.chat_log_system(&msg);
                    game::CommandAck::fail(msg)
                }
            }
            game::hermes::GameCommand::Read { id } => {
                // Open volume on the lectern — fetch and display
                match self.bfs.get_volume(&id).await {
                    Ok(vol) => {
                        let title = vol.conventional_name.clone()
                            .unwrap_or_else(|| id.clone());
                        let mut sentences: Vec<(Option<String>, Option<String>)> = Vec::new();
                        sentences.push((Some(title.clone()), None));
                        sentences.push((None, None));
                        for s in &vol.sentences {
                            let trans = s.translation.get("en")
                                .or_else(|| s.translation.get("de"))
                                .cloned();
                            sentences.push((s.transliteration.clone(), trans));
                        }
                        if vol.sentences.is_empty() {
                            sentences.push((None, Some("(no sentence data available)".to_string())));
                        }
                        // Save chat state before switching popup
                        self.save_chat_state();
                        self.active_popup = Some(PopupContent::LecternRead {
                            title,
                            sentences,
                            scroll_offset: 0,
                        });
                        game::CommandAck::ok()
                    }
                    Err(e) => {
                        let msg = format!("Cannot read {}: {}", id, e);
                        self.chat_log_system(&msg);
                        game::CommandAck::fail(msg)
                    }
                }
            }
            game::hermes::GameCommand::Gather { id } => {
                let room = self.engine.current_room;
                let turn = self.engine.turn;
                let district = self.world.rooms.get(&room)
                    .map(|r| r.info.district.clone())
                    .unwrap_or_default();
                let ptype = self.world.rooms.get(&room)
                    .and_then(|r| r.info.volumes_typed.iter().find(|v| v.id == id))
                    .map(|v| v.physical_type.clone())
                    .unwrap_or_else(|| "unknown".into());
                if self.inventory.gather(id.clone(), room, turn, id.clone(), ptype.clone(), district) {
                    self.spawn_gather_vortex_hermes(&id, &id, &ptype, 0);
                    self.chat_log_system(&format!("Gathered: {}", id));
                    game::CommandAck::ok()
                } else {
                    self.chat_log_system(&format!("{} already in your collection", id));
                    game::CommandAck::fail("already in collection")
                }
            }
            game::hermes::GameCommand::GatherBatch { ids } => {
                let room = self.engine.current_room;
                let turn = self.engine.turn;
                let district = self.world.rooms.get(&room)
                    .map(|r| r.info.district.clone())
                    .unwrap_or_default();
                let mut gathered = 0u32;
                let mut dupes = 0u32;
                for (batch_idx, id) in ids.iter().enumerate() {
                    let ptype = self.world.rooms.get(&room)
                        .and_then(|r| r.info.volumes_typed.iter().find(|v| &v.id == id))
                        .map(|v| v.physical_type.clone())
                        .unwrap_or_else(|| "unknown".into());
                    if self.inventory.gather(id.clone(), room, turn, id.clone(), ptype.clone(), district.clone()) {
                        // Stagger each vortex by 12 ticks so they don't all fire simultaneously
                        self.spawn_gather_vortex_hermes(id, id, &ptype, batch_idx as u64 * 12);
                        gathered += 1;
                    } else {
                        dupes += 1;
                    }
                }
                if gathered > 0 {
                    self.chat_log_system(&format!("Gathered {} texts", gathered));
                    if dupes > 0 {
                        game::CommandAck { status: "ok", reason: Some(format!("{} new, {} already collected", gathered, dupes)) }
                    } else {
                        game::CommandAck::ok()
                    }
                } else {
                    self.chat_log_system("All texts already in your collection");
                    game::CommandAck::fail("all already in collection")
                }
            }
            game::hermes::GameCommand::Scribe { volumes, topic } => {
                if volumes.is_empty() {
                    self.chat_log_system("No texts to give the Scribe.");
                    return game::CommandAck::fail("no volumes provided");
                }
                self.spawn_scribe();
                match ScribeBridge::spawn(&volumes, &topic).await {
                    Ok(bridge) => {
                        self.scribe_bridge = Some(bridge);
                        self.scribe_status = ScribeStatus::Fetching;
                        self.scribe_streaming.clear();
                        self.scribe_reasoning_ring.clear();
                        // Spawn ritual ring around the Scribe NPC
                        if let Some(scribe) = self.engine.scribes.first() {
                            self.ritual_ring = Some(render::ritual::RitualRing::new(
                                scribe.coord(),
                                self.engine.current_room,
                                self.tick,
                            ));
                        }
                        self.chat_log_system(&format!(
                            "The Scribe receives {} texts. Writing begins.",
                            volumes.len()
                        ));
                        self.open_scribe_chat();
                        game::CommandAck::ok()
                    }
                    Err(e) => {
                        let msg = format!("Scribe unavailable: {}", e);
                        self.chat_log_system(&msg);
                        game::CommandAck::fail(msg)
                    }
                }
            }
        }
    }

    /// Execute a game command from fenced JSON (backward compat — discards ack).
    async fn execute_game_command(&mut self, cmd: game::hermes::GameCommand) {
        let _ack = self.execute_game_command_with_ack(cmd).await;
    }

    /// Save current chat panel state to ChatState for later restoration.
    fn save_chat_state(&mut self) {
        if let Some(PopupContent::Chat {
            ref messages,
            ref input_buffer,
            ref input_cursor,
            ref scroll_offset,
            ..
        }) = self.active_popup {
            self.chat_state = Some(ChatState {
                messages: messages.clone(),
                input_buffer: input_buffer.clone(),
                input_cursor: *input_cursor,
                scroll_offset: *scroll_offset,
            });
        }
    }

    /// Log a system message to both the active chat popup AND persistent chat_state.
    /// Works whether chat is open (popup) or minimized (sidebar reads from chat_state).
    fn chat_log_system(&mut self, text: &str) {
        let msg = ChatMessage::new(ChatRole::System, text.to_string());
        if let Some(PopupContent::Chat { ref mut messages, ref mut scroll_offset, .. }) = self.active_popup {
            messages.push(msg.clone());
            *scroll_offset = 0;
        }
        // Always update persistent state so sidebar sees it
        if let Some(ref mut cs) = self.chat_state {
            cs.messages.push(msg);
        }
    }

    /// Commit accumulated streaming_partial to messages (popup + chat_state + sidebar).
    /// Called at phase transitions (reasoning→content, content→tool) so text isn't lost.
    fn commit_streaming_partial(&mut self) {
        // Commit popup streaming_partial
        if let Some(PopupContent::Chat { ref mut streaming_partial, ref mut streaming_is_thinking, ref mut messages, .. }) = self.active_popup {
            if !streaming_partial.trim().is_empty() {
                let was_thinking = *streaming_is_thinking;
                let text = std::mem::take(streaming_partial);
                if was_thinking {
                    messages.push(ChatMessage::thinking(ChatRole::Librarian, text));
                } else {
                    messages.push(ChatMessage::new(ChatRole::Librarian, text));
                }
                *streaming_is_thinking = false;
            } else {
                streaming_partial.clear();
                *streaming_is_thinking = false;
            }
        }
        // Commit sidebar streaming → persistent chat_state
        if !self.sidebar_streaming.trim().is_empty() {
            let committed = std::mem::take(&mut self.sidebar_streaming);
            if let Some(ref mut cs) = self.chat_state {
                // Sidebar doesn't track thinking state separately — mark as thinking
                // if the chat_reasoning_buf was non-empty (heuristic).
                let is_think = !self.chat_reasoning_buf.is_empty();
                if is_think {
                    cs.messages.push(ChatMessage::thinking(ChatRole::Librarian, committed));
                } else {
                    cs.messages.push(ChatMessage::new(ChatRole::Librarian, committed));
                }
            }
        } else {
            self.sidebar_streaming.clear();
        }
    }

    /// Scan articles directory for .md files and build article list.
    fn scan_articles() -> Vec<render::popup::ArticleEntry> {
        let dir = format!("{}/content", articles_dir());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };

        let mut articles: Vec<render::popup::ArticleEntry> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let Ok(content) = std::fs::read_to_string(&path) else { continue };

                // Extract title from first # heading
                let title = content.lines()
                    .find(|l| l.starts_with("# "))
                    .map(|l| l.trim_start_matches('#').trim().trim_matches('"').to_string())
                    .unwrap_or_else(|| filename.trim_end_matches(".md").replace('-', " "));

                let word_count = content.split_whitespace().count() as u32;

                // Date from file metadata
                let date = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .map(|t| {
                        let elapsed = t.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap_or_default();
                        let secs = elapsed.as_secs();
                        // Simple date formatting without chrono
                        let days = secs / 86400;
                        let years = 1970 + days / 365;
                        let day_of_year = days % 365;
                        let month = day_of_year / 30 + 1;
                        let day = day_of_year % 30 + 1;
                        format!("{:04}-{:02}-{:02}", years, month.min(12), day.min(31))
                    })
                    .unwrap_or_default();

                articles.push(render::popup::ArticleEntry { filename, title, word_count, date });
            }
        }
        // Sort by date descending (newest first)
        articles.sort_by(|a, b| b.date.cmp(&a.date));
        articles
    }

    /// Parse "q,r,z" and look up room via BFS.
    async fn lookup_room_coords(&self, input: &str) -> render::popup::RoomLookupResult {
        let parts: Vec<&str> = input.split(|c: char| c == ',' || c == ' ')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if parts.len() != 3 {
            return render::popup::RoomLookupResult {
                coord: [0, 0, 0],
                name: String::new(),
                district: String::new(),
                era: String::new(),
                volume_count: 0,
                composition: String::new(),
                cluster_sig: Vec::new(),
                error: Some("Enter 3 values: q,r,z".to_string()),
            };
        }

        let parsed: Result<Vec<i32>, _> = parts.iter().map(|s| s.parse::<i32>()).collect();
        let coords = match parsed {
            Ok(v) => [v[0], v[1], v[2]],
            Err(_) => {
                return render::popup::RoomLookupResult {
                    coord: [0, 0, 0],
                    name: String::new(),
                    district: String::new(),
                    era: String::new(),
                    volume_count: 0,
                    composition: String::new(),
                    cluster_sig: Vec::new(),
                    error: Some("Invalid numbers".to_string()),
                };
            }
        };

        match self.bfs.get_room(coords[0], coords[1], coords[2]).await {
            Ok(meta) => {
                let name = room_name_from_cluster(
                    &meta.cluster_id,
                    &meta.cluster_signature,
                    &meta.room_type,
                );
                let district = display_district(&meta.district).to_string();
                let era = era_name_from_z(meta.z).to_string();
                let comp = RoomComposition::from_volumes_typed(&meta.volumes_typed);
                let composition = composition_summary(&comp);
                render::popup::RoomLookupResult {
                    coord: coords,
                    name,
                    district,
                    era,
                    volume_count: meta.volume_count,
                    composition,
                    cluster_sig: meta.cluster_signature.clone(),
                    error: None,
                }
            }
            Err(_) => {
                render::popup::RoomLookupResult {
                    coord: coords,
                    name: String::new(),
                    district: String::new(),
                    era: String::new(),
                    volume_count: 0,
                    composition: String::new(),
                    cluster_sig: Vec::new(),
                    error: Some(format!("Room [{},{},{}] not found", coords[0], coords[1], coords[2])),
                }
            }
        }
    }

    /// Preload rooms visible in the fullscreen map view.
    /// Collects missing coords in viewport, batch-fetches in one RPC call.
    /// Collect missing map viewport coords and spawn background fetch.
    fn spawn_preload_map_visible(&mut self) {
        let map_center = if let Some(PopupContent::PauseMenu { ref map_center, .. }) = self.active_popup {
            *map_center
        } else { return; };

        let q_range = 12i32;
        let r_range = 8i32;
        let z = map_center[2];
        let cq = map_center[0];
        let cr = map_center[1];

        const MAX_FETCHES: usize = 48;
        let mut to_fetch: Vec<[i32; 3]> = Vec::new();

        for dq in -q_range..=q_range {
            for dr in -r_range..=r_range {
                let coord = [cq + dq, cr + dr, z];
                if self.world.rooms.contains_key(&coord) { continue; }
                if self.bfs_miss_cache.contains(&coord) { continue; }
                to_fetch.push(coord);
                if to_fetch.len() >= MAX_FETCHES { break; }
            }
            if to_fetch.len() >= MAX_FETCHES { break; }
        }

        if to_fetch.is_empty() {
            return;
        }

        let bfs = self.bfs.clone();
        let tx = self.preload_tx.clone();
        self.preload_in_flight = true;
        tokio::spawn(async move {
            match bfs.get_rooms(&to_fetch).await {
                Ok(results) => {
                    for (coord, result) in to_fetch.iter().zip(results.into_iter()) {
                        let msg = match result {
                            Some(meta) => PreloadResult::Room(meta),
                            None => PreloadResult::Miss(*coord),
                        };
                        let _ = tx.send(msg).await;
                    }
                }
                Err(_) => {}
            }
            let _ = tx.send(PreloadResult::Done).await;
        });
    }

    /// Navigate to a room by coordinate — teleport player and load surroundings.
    /// Clears any active portal (keyboard navigation bypasses portal flow).
    async fn navigate_to_room(&mut self, coord: [i32; 3]) {
        self.world.clear_active_portal();

        // Load target room if not already loaded
        if !self.world.rooms.contains_key(&coord) {
            if let Ok(meta) = self.bfs.get_room(coord[0], coord[1], coord[2]).await {
                self.world.place_room(&meta);
            } else {
                self.status_message = Some(format!("Cannot reach [{},{},{}]", coord[0], coord[1], coord[2]));
                return;
            }
        }

        if let Some(room) = self.world.rooms.get(&coord) {
            self.engine.player.pos.coord = room.center;
            self.engine.current_room = coord;
            self.engine.teleport_librarian_to_player(&self.world);
            self.engine.teleport_scribe_to_librarian(&self.world);

            self.current_palette = Palette::from_room(room.info.z, &room.info.district);
            let atmo = atmospheric_text(
                &room.info.room_type,
                &room.info.district,
                room.info.volume_count,
                room.info.z,
            );
            self.atmosphere_text = Some(atmo.to_string());
            self.atmosphere_expires = Some(Instant::now() + std::time::Duration::from_secs(4));
            let comp = RoomComposition::from_volumes_typed(&room.info.volumes_typed);
            self.composition_cache = composition_summary(&comp);

            if !self.room_particles.contains_key(&coord) {
                let rp = init_room_particles(
                    coord, &self.world.tiles, room.info.z,
                    &room.info.district, room.info.volume_count as usize,
                );
                self.room_particles.insert(coord, rp);
            }

            if self.breadcrumb_trail.back() != Some(&coord) {
                self.breadcrumb_trail.push_back(coord);
                if self.breadcrumb_trail.len() > 8 { self.breadcrumb_trail.pop_front(); }
            }

            self.spawn_preload_proximity();
            self.status_message = None;
        }
    }

    /// Handle stepping onto a portal tile — teleport to origin or destination.
    async fn handle_portal_enter(&mut self) {
        let player_pos = self.engine.player.coord();

        // Determine direction: origin → dest, or dest → origin
        let target_coord = if let Some(ref portal) = self.world.active_portal {
            if portal.origin_tiles.contains(&player_pos) {
                Some(portal.dest_room)
            } else if portal.dest_tiles.contains(&player_pos) {
                Some(portal.origin_room)
            } else {
                None
            }
        } else {
            None
        };

        let Some(coord) = target_coord else {
            // Orphan portal tile (no active_portal or not in either set).
            // Sync current_room from the tile the player is standing on.
            if let Some(wt) = self.world.tiles.get(&player_pos) {
                self.engine.current_room = wt.room_coord;
            }
            return;
        };

        // Ensure destination is loaded
        if !self.world.rooms.contains_key(&coord) {
            if let Ok(meta) = self.bfs.get_room(coord[0], coord[1], coord[2]).await {
                self.world.place_room(&meta);
            } else {
                self.status_message = Some(format!("Cannot reach [{},{},{}]", coord[0], coord[1], coord[2]));
                return;
            }
        }

        // Extract room data before mutable borrows
        let room_data = self.world.rooms.get(&coord).map(|room| {
            (room.center, room.info.z, room.info.district.clone(),
             room.info.room_type.clone(), room.info.volume_count,
             room.info.volumes_typed.clone())
        });

        if let Some((center, z, district, _room_type, volume_count, volumes_typed)) = room_data {
            self.engine.player.pos.coord = center;
            self.engine.current_room = coord;
            self.world.touch_room(coord);
            self.engine.teleport_librarian_to_player(&self.world);
            self.engine.teleport_scribe_to_librarian(&self.world);

            self.current_palette = Palette::from_room(z, &district);
            self.atmosphere_text = Some("You step through the portal...".to_string());
            self.atmosphere_expires = Some(Instant::now() + std::time::Duration::from_secs(4));
            let comp = RoomComposition::from_volumes_typed(&volumes_typed);
            self.composition_cache = composition_summary(&comp);

            if !self.room_particles.contains_key(&coord) {
                let rp = init_room_particles(
                    coord, &self.world.tiles, z,
                    &district, volume_count as usize,
                );
                self.room_particles.insert(coord, rp);
            }

            if self.breadcrumb_trail.back() != Some(&coord) {
                self.breadcrumb_trail.push_back(coord);
                if self.breadcrumb_trail.len() > 8 { self.breadcrumb_trail.pop_front(); }
            }

            self.spawn_preload_proximity();
            self.status_message = None;

            // Write tile grid for agent pathfinding
            game::agent_state::write_tile_grid(
                &self.world,
                self.engine.player.coord(),
                self.engine.current_room,
            );
        }
    }

    /// Spawn the Scribe NPC behind the Librarian (if not already present).
    fn spawn_scribe(&mut self) {
        if self.engine.scribes.is_empty() {
            if let Some(lib) = self.engine.librarians.first() {
                let lib_coord = lib.coord();
                let lib_facing = lib.facing();
                let mut scribe = game::Actor::new(
                    game::ActorKind::Scribe,
                    lib_coord,
                    lib_facing,
                );
                game::librarian::teleport_behind_player(
                    &mut scribe,
                    lib_coord,
                    lib_facing,
                    &self.world,
                );
                self.engine.scribes.push(scribe);
            }
        }
    }

    /// Open or update the ScribeChat popup, preserving Librarian chat state.
    fn open_scribe_chat(&mut self) {
        // Save Librarian chat state before replacing popup
        self.save_chat_state();

        let title = if let Some(ref bridge) = self.scribe_bridge {
            if bridge.article_title.is_empty() {
                "The Scribe".to_string()
            } else {
                bridge.article_title.clone()
            }
        } else {
            "The Scribe".to_string()
        };

        let word_count = self.scribe_bridge.as_ref().map(|b| b.word_count).unwrap_or(0);

        let (model, vol_count, pass_count) = if let Some(ref bridge) = self.scribe_bridge {
            (bridge.model.clone(), bridge.volume_count, bridge.passage_count)
        } else {
            (String::new(), 0, 0)
        };

        self.active_popup = Some(PopupContent::ScribeChat {
            title,
            streaming_text: self.scribe_streaming.clone(),
            status: self.scribe_status.clone(),
            status_text: self.scribe_status_text.clone(),
            scroll_offset: 0,
            word_count,
            model,
            volume_count: vol_count,
            passage_count: pass_count,
            reasoning_ring: self.scribe_reasoning_ring.clone(),
        });
    }

    /// Handle a scribe event from the subprocess bridge.
    fn handle_scribe_event(&mut self, event: ScribeEvent) {
        match event {
            ScribeEvent::Status { text } => {
                self.scribe_status_text = Some(text);
            }
            ScribeEvent::Meta { title, volume_count, passage_count } => {
                if let Some(ref mut bridge) = self.scribe_bridge {
                    bridge.article_title = title;
                    bridge.volume_count = volume_count;
                    bridge.passage_count = passage_count;
                    bridge.status = ScribeStatus::Writing;
                }
                self.scribe_status = ScribeStatus::Writing;
            }
            ScribeEvent::ModelSwitch { name } => {
                if let Some(ref mut bridge) = self.scribe_bridge {
                    bridge.model = name;
                }
                // Clear streaming state on model switch (retry after truncation)
                self.scribe_streaming.clear();
                self.scribe_reasoning_ring.clear();
            }
            ScribeEvent::Reasoning { text } => {
                self.scribe_status = ScribeStatus::Reasoning;
                self.scribe_status_text = None;
                if let Some(ref mut ritual) = self.ritual_ring {
                    ritual.advance_to(render::ritual::RitualPhase::Channeling, self.tick);
                }
                // Assemble reasoning chunks into lines
                if self.scribe_reasoning_ring.is_empty() {
                    self.scribe_reasoning_ring.push(String::new());
                }
                for (i, part) in text.split('\n').enumerate() {
                    if i > 0 {
                        self.scribe_reasoning_ring.push(String::new());
                    }
                    if let Some(last) = self.scribe_reasoning_ring.last_mut() {
                        last.push_str(part);
                    }
                }
                // Cap ring buffer
                while self.scribe_reasoning_ring.len() > 50 {
                    self.scribe_reasoning_ring.remove(0);
                }
            }
            ScribeEvent::Delta { text } => {
                self.scribe_streaming.push_str(&text);
                self.scribe_status = ScribeStatus::Writing;
                self.scribe_status_text = None;
                if let Some(ref mut ritual) = self.ritual_ring {
                    ritual.advance_to(render::ritual::RitualPhase::Contracting, self.tick);
                }
            }
            ScribeEvent::Complete { text, word_count, file } => {
                let final_text = if self.scribe_streaming.is_empty() {
                    text
                } else {
                    std::mem::take(&mut self.scribe_streaming)
                };
                if let Some(ref mut bridge) = self.scribe_bridge {
                    bridge.article_text = final_text.clone();
                    bridge.word_count = word_count;
                    bridge.article_file = file;
                    bridge.status = ScribeStatus::Complete;
                }
                self.scribe_status = ScribeStatus::Complete;
                self.scribe_streaming = final_text;
                if let Some(ref mut ritual) = self.ritual_ring {
                    ritual.advance_to(render::ritual::RitualPhase::Releasing, self.tick);
                }

                // Clear inventory after successful synthesis
                self.inventory.clear();

                // Notification
                let title = self.scribe_bridge.as_ref()
                    .map(|b| b.article_title.clone())
                    .unwrap_or_default();
                self.chat_log_system(&format!(
                    "The Scribe has finished: \"{}\" \u{2014} {} words.", title, word_count
                ));
            }
            ScribeEvent::Error { message } => {
                if message != "Scribe process ended" || !matches!(self.scribe_status, ScribeStatus::Complete) {
                    self.scribe_status = ScribeStatus::Error(message);
                    if let Some(ref mut ritual) = self.ritual_ring {
                        ritual.advance_to(render::ritual::RitualPhase::Fizzling, self.tick);
                    }
                }
            }
        }

        // Update ScribeChat popup if open
        if let Some(PopupContent::ScribeChat {
            ref mut streaming_text,
            ref mut status,
            ref mut status_text,
            ref mut word_count,
            ref mut volume_count,
            ref mut passage_count,
            ref mut reasoning_ring,
            ..
        }) = self.active_popup {
            *streaming_text = self.scribe_streaming.clone();
            *status = self.scribe_status.clone();
            *status_text = self.scribe_status_text.clone();
            *reasoning_ring = self.scribe_reasoning_ring.clone();
            if let Some(ref bridge) = self.scribe_bridge {
                *word_count = bridge.word_count;
                *volume_count = bridge.volume_count;
                *passage_count = bridge.passage_count;
            }
        }
    }

    /// Attempt to respawn the hermes bridge after disconnect or overflow.
    async fn respawn_bridge(&mut self) {
        // Drop old bridge
        if let Some(mut old) = self.hermes_bridge.take() {
            old.close().await;
        }
        // Spawn fresh in background
        self.status_message = Some("Reconnecting to Librarian...".into());
        self.bridge_pending = Some(tokio::spawn(async {
            let mut bridge = HermesBridge::spawn().await.map_err(|e| format!("{}", e))?;
            bridge.create_session(80).await.map_err(|e| format!("{}", e))?;
            Ok(bridge)
        }));
    }
}

/// Resolve articles directory from ARTICLES_DIR env, falling back to ./articles in repo root.
fn articles_dir() -> String {
    std::env::var("ARTICLES_DIR").unwrap_or_else(|_| {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let root = std::path::Path::new(manifest)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        format!("{}/articles", root)
    })
}

/// Collect all neighbor room coords from a room's metadata (doors + shafts).
fn room_neighbor_coords(meta: &RoomMetadata) -> Vec<[i32; 3]> {
    let mut coords: Vec<[i32; 3]> = meta.door_edges.iter().map(|e| e.to_coord).collect();
    coords.extend(meta.shaft_edges.iter().map(|e| e.to_coord));
    coords
}

fn room_info_neighbor_coords(info: &game::world::RoomInfo) -> Vec<[i32; 3]> {
    let mut coords: Vec<[i32; 3]> = info.door_edges.iter().map(|e| e.to_coord).collect();
    coords.extend(info.shaft_edges.iter().map(|e| e.to_coord));
    coords
}

/// BFS-walk the room graph starting from `start`, collecting up to `max` rooms.
/// Traverses both horizontal (door) edges and vertical (shaft) edges for z-connectivity.
async fn bfs_collect_rooms(
    bfs: &BfsClient,
    start: &RoomMetadata,
    max: usize,
) -> Vec<RoomMetadata> {
    let mut collected: Vec<RoomMetadata> = vec![start.clone()];
    let mut visited: HashSet<[i32; 3]> = HashSet::new();
    visited.insert([start.q, start.r, start.z]);

    let mut queue: VecDeque<[i32; 3]> = VecDeque::new();

    // Seed queue with start's horizontal + vertical neighbors
    enqueue_neighbors(&start.door_edges, &start.shaft_edges, &mut queue, &mut visited);

    // Batch-fetch in waves until we hit the cap
    while !queue.is_empty() && collected.len() < max {
        let remaining = max - collected.len();
        let batch: Vec<[i32; 3]> = queue.drain(..queue.len().min(remaining)).collect();

        match bfs.get_rooms(&batch).await {
            Ok(results) => {
                for result in results.into_iter().flatten() {
                    enqueue_neighbors(&result.door_edges, &result.shaft_edges, &mut queue, &mut visited);
                    collected.push(result);
                }
            }
            Err(_) => break,
        }
    }

    collected
}

/// Enqueue all reachable neighbors (horizontal doors + vertical shafts).
fn enqueue_neighbors(
    door_edges: &[bfs_client::DoorEdge],
    shaft_edges: &[bfs_client::ShaftEdge],
    queue: &mut VecDeque<[i32; 3]>,
    visited: &mut HashSet<[i32; 3]>,
) {
    for edge in door_edges {
        if !visited.contains(&edge.to_coord) {
            queue.push_back(edge.to_coord);
            visited.insert(edge.to_coord);
        }
    }
    for edge in shaft_edges {
        if !visited.contains(&edge.to_coord) {
            queue.push_back(edge.to_coord);
            visited.insert(edge.to_coord);
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Panic hook: restore terminal + clean up bridge on panic
    std::panic::set_hook(Box::new(|info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            crossterm::cursor::Show,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0)
        );
        {
            use std::io::Write;
            let mut out = io::stdout();
            let _ = write!(out, "\x1B[2J\x1B[H");
            let _ = out.flush();
        }
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", "stacks-hermes"])
            .output();
        let _ = std::process::Command::new("fuser")
            .args(["-k", "/tmp/stacks-hermes.in", "/tmp/stacks-hermes.out"])
            .stderr(std::process::Stdio::null())
            .output();
        let _ = std::fs::remove_file("/tmp/stacks-hermes.in");
        let _ = std::fs::remove_file("/tmp/stacks-hermes.out");
        let _ = std::fs::remove_file("/tmp/stacks-game.sock");
        eprintln!("PANIC: {}", info);
    }));

    // Signal handler: SIGTERM/SIGHUP trigger graceful shutdown
    let quit_signal = Arc::new(AtomicBool::new(false));
    let qs = quit_signal.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        let mut sighup = signal(SignalKind::hangup()).expect("SIGHUP handler");
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sighup.recv() => {},
        }
        qs.store(true, Ordering::Relaxed);
    });

    // Force full terminal redraw every frame (for asciinema recording).
    // Without this, ratatui's diff optimization only emits changed cells,
    // which causes missing characters when agg chunks the cast file.
    let full_redraw = std::env::var("STACKS_FULL_REDRAW").is_ok();

    // ── Splash screens: "Powered by Hermes" then "Powered by Kimi" ──
    {
        use std::time::Duration;

        // Hermes caduceus (from hermes-agent banner.py, Rich markup stripped)
        const HERMES_ART: &[&str] = &[
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⡀⠀⣀⣀⠀⢀⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⢀⣠⣴⣾⣿⣿⣇⠸⣿⣿⠇⣸⣿⣿⣷⣦⣄⡀⠀⠀⠀⠀⠀⠀",
            "⠀⢀⣠⣴⣶⠿⠋⣩⡿⣿⡿⠻⣿⡇⢠⡄⢸⣿⠟⢿⣿⢿⣍⠙⠿⣶⣦⣄⡀⠀",
            "⠀⠀⠉⠉⠁⠶⠟⠋⠀⠉⠀⢀⣈⣁⡈⢁⣈⣁⡀⠀⠉⠀⠙⠻⠶⠈⠉⠉⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣴⣿⡿⠛⢁⡈⠛⢿⣿⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠿⣿⣦⣤⣈⠁⢠⣴⣿⠿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠉⠻⢿⣿⣦⡉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠘⢷⣦⣈⠛⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣴⠦⠈⠙⠿⣦⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠸⣿⣤⡈⠁⢤⣿⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠛⠷⠄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣀⠑⢶⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⠁⢰⡆⠈⡿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠳⠈⣡⠞⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
        ];

        // Kimi crescent moon (generated via braille pixel rasterization)
        // Kimi crescent moon (high-contrast render → ascii-image-converter braille)
        const KIMI_ART: &[&str] = &[
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣀⣀⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⣶⣾⣿⣿⣿⣿⣿⣿⣷⣶⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⢀⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⠿⠿⠿⢦⡀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⣠⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠋⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⣰⣿⣿⣿⣿⣿⣿⣿⣿⣿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⢠⣿⣿⣿⣿⣿⣿⣿⣿⣿⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⣼⣿⣿⣿⣿⣿⣿⣿⣿⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⣿⣿⣿⣿⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⣿⣿⣿⣿⣿⣿⣿⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⢿⣿⣿⣿⣿⣿⣿⣿⣿⡇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠘⣿⣿⣿⣿⣿⣿⣿⣿⣿⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠹⣿⣿⣿⣿⣿⣿⣿⣿⣿⣄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠹⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠈⠻⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣶⣶⠞⠁⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠈⠙⠿⣿⣿⣿⣿⣿⣿⣿⣿⠿⠛⠁⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠉⠉⠉⠉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
            "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
        ];

        struct SplashCard {
            art: &'static [&'static str],
            label: &'static str,
            target_r: u8, target_g: u8, target_b: u8,
            label_r: u8, label_g: u8, label_b: u8,
        }

        let cards = [
            SplashCard {
                art: HERMES_ART,
                label: "Powered by Hermes",
                target_r: 255, target_g: 191, target_b: 0,   // #FFBF00 amber/gold
                label_r: 255, label_g: 215, label_b: 0,      // #FFD700 gold
            },
            SplashCard {
                art: KIMI_ART,
                label: "Powered by Kimi",
                target_r: 140, target_g: 180, target_b: 255,  // soft blue moonlight
                label_r: 200, label_g: 220, label_b: 255,     // brighter blue-white
            },
        ];

        let fade_in_ms = 2000u64;
        let hold_ms = 2000u64;
        let fade_out_ms = 2000u64;
        let total_ms = fade_in_ms + hold_ms + fade_out_ms;

        for card in &cards {
            let start = Instant::now();
            let mut skipped = false;

            loop {
                let elapsed = start.elapsed().as_millis() as u64;
                if elapsed >= total_ms {
                    break;
                }

                // Check for keypress to skip
                if event::poll(Duration::from_millis(16))? {
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            skipped = true;
                            break;
                        }
                    }
                }

                // Calculate fade alpha (0.0 to 1.0)
                let alpha = if elapsed < fade_in_ms {
                    elapsed as f64 / fade_in_ms as f64
                } else if elapsed < fade_in_ms + hold_ms {
                    1.0
                } else {
                    1.0 - ((elapsed - fade_in_ms - hold_ms) as f64 / fade_out_ms as f64)
                };
                let a = alpha.clamp(0.0, 1.0);

                let art_color = Color::Rgb(
                    (card.target_r as f64 * a) as u8,
                    (card.target_g as f64 * a) as u8,
                    (card.target_b as f64 * a) as u8,
                );
                let label_color = Color::Rgb(
                    (card.label_r as f64 * a) as u8,
                    (card.label_g as f64 * a) as u8,
                    (card.label_b as f64 * a) as u8,
                );

                terminal.draw(|frame| {
                    let area = frame.area();
                    // Black background
                    frame.render_widget(
                        Block::default().style(Style::default().bg(Color::Black)),
                        area,
                    );

                    let art_lines = card.art.len() as u16;
                    let label_height = 2u16; // gap + label
                    let total_h = art_lines + label_height;
                    let start_y = area.y + area.height.saturating_sub(total_h) / 2;

                    // Render art lines centered
                    for (i, line) in card.art.iter().enumerate() {
                        let line_width = line.chars().count() as u16;
                        let x = area.x + area.width.saturating_sub(line_width) / 2;
                        let y = start_y + i as u16;
                        if y < area.y + area.height {
                            let span = Span::styled(*line, Style::default().fg(art_color));
                            frame.render_widget(
                                Paragraph::new(Line::from(span)),
                                Rect::new(x, y, line_width, 1),
                            );
                        }
                    }

                    // Render label centered below art
                    let label_y = start_y + art_lines + 1;
                    if label_y < area.y + area.height {
                        let label_width = card.label.len() as u16;
                        let lx = area.x + area.width.saturating_sub(label_width) / 2;
                        let span = Span::styled(card.label, Style::default().fg(label_color).add_modifier(Modifier::BOLD));
                        frame.render_widget(
                            Paragraph::new(Line::from(span)),
                            Rect::new(lx, label_y, label_width, 1),
                        );
                    }
                })?;
            }

            // If user skipped, skip remaining cards too
            if skipped {
                break;
            }
        }

        // Brief black frame before game starts
        terminal.draw(|frame| {
            frame.render_widget(
                Block::default().style(Style::default().bg(Color::Black)),
                frame.area(),
            );
        })?;
        std::thread::sleep(Duration::from_millis(200));
    }

    let mut app = App::new().await;

    // Initial tile grid dump for agent pathfinding
    game::agent_state::write_tile_grid(
        &app.world, app.engine.player.coord(), app.engine.current_room,
    );

    // Spawn hermes bridge in background (non-fatal — game works without it)
    app.status_message = Some("Connecting to Librarian...".into());
    app.bridge_pending = Some(tokio::spawn(async {
        let mut bridge = HermesBridge::spawn().await.map_err(|e| format!("{}", e))?;
        bridge.create_session(80).await.map_err(|e| format!("{}", e))?;
        Ok(bridge)
    }));

    loop {
        // Check if background bridge spawn completed
        if let Some(ref handle) = app.bridge_pending {
            if handle.is_finished() {
                let handle = app.bridge_pending.take().unwrap();
                match handle.await {
                    Ok(Ok(bridge)) => {
                        app.status_message = Some("Librarian connected".into());
                        app.hermes_bridge = Some(bridge);
                        // Update chat UI if reconnecting
                        if let Some(PopupContent::Chat { ref mut messages, ref mut status, .. }) = app.active_popup {
                            if matches!(status, ChatStatus::WaitingForAgent) {
                                messages.push(ChatMessage::new(ChatRole::System, "The Librarian returns.".to_string()));
                                *status = ChatStatus::Idle;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let msg = format!("Hermes: {}", e);
                        eprintln!("[stacks] {}", msg);
                        app.status_message = Some(msg);
                        if let Some(PopupContent::Chat { ref mut messages, ref mut status, .. }) = app.active_popup {
                            messages.push(ChatMessage::new(ChatRole::System, "Could not reconnect. Close and reopen to retry.".to_string()));
                            *status = ChatStatus::Idle;
                        }
                    }
                    Err(e) => {
                        let msg = format!("Hermes task panic: {}", e);
                        eprintln!("[stacks] {}", msg);
                        app.status_message = Some(msg);
                    }
                }
            }
        }

        // Drain hermes events (non-blocking) — collect first to avoid double borrow
        let mut hermes_events = Vec::new();
        if let Some(ref mut bridge) = app.hermes_bridge {
            while let Some(event) = bridge.try_recv() {
                hermes_events.push(event);
            }
        }
        for event in hermes_events {
            app.handle_hermes_event(event).await;
        }

        // Drain game commands from stacks-game.py socket (non-blocking)
        app.drain_command_requests().await;

        // Drain scribe events (non-blocking)
        let mut scribe_events = Vec::new();
        if let Some(ref mut bridge) = app.scribe_bridge {
            while let Some(event) = bridge.try_recv() {
                scribe_events.push(event);
            }
        }
        for event in scribe_events {
            app.handle_scribe_event(event);
        }

        // Expire atmospheric text
        if let Some(expires) = app.atmosphere_expires {
            if Instant::now() > expires {
                app.atmosphere_text = None;
                app.atmosphere_expires = None;
            }
        }

        // Expire gather toasts
        let now = Instant::now();
        app.gather_toasts.retain(|t| t.3 > now);

        // Tick atmosphere systems
        app.tick = app.tick.wrapping_add(1);

        // Update particles for current room
        let current_room = app.engine.current_room;
        if let Some(rp) = app.room_particles.get_mut(&current_room) {
            tick_particles(rp, &app.world.tiles, app.tick);
        }

        // Tick ritual ring animation (clear if player left the room)
        if let Some(ref mut ritual) = app.ritual_ring {
            if ritual.room_coord != app.engine.current_room || !ritual.tick(app.tick) {
                app.ritual_ring = None;
            }
        }

        // Tick gather vortex animations — remove completed ones and fire deferred toasts
        {
            let player_pos = app.engine.player.coord();
            let current_room = app.engine.current_room;
            let tick = app.tick;
            let mut completed_toasts = Vec::new();
            app.gather_vortices.retain_mut(|vortex| {
                // Remove vortices from other rooms
                if vortex.room_coord != current_room {
                    return false;
                }
                match vortex.tick(player_pos, tick) {
                    VortexStatus::Running => true,
                    VortexStatus::Complete(toast) => {
                        completed_toasts.push(toast);
                        false
                    }
                }
            });
            for toast in completed_toasts {
                app.push_gather_toast(&toast.name, &toast.physical_type);
            }
        }

        // Collect light sources for this frame
        let player_pos_for_light = app.engine.player.coord();
        let mut light_sources = collect_light_sources(&app.world.tiles, player_pos_for_light);

        // Add vortex light sources (warm glow follows the traveling particle)
        for vortex in &app.gather_vortices {
            if let Some(pos) = vortex.light_position(player_pos_for_light) {
                light_sources.push(render::lighting::LightSource {
                    pos,
                    radius: 4,
                    temp_r: 30,
                    temp_g: 20,
                    temp_b: -5,
                });
            }
        }

        terminal.draw(|frame| {
            let area = frame.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                ])
                .split(area);

            // --- Title bar: all location info ---
            let coord = app.engine.current_room;
            let room_meta = app.world.rooms.get(&coord);
            let title = if let Some(room) = room_meta {
                let room_name = room_name_from_cluster(
                    &room.info.cluster_id,
                    &room.info.cluster_signature,
                    &room.info.room_type,
                );
                let dist = display_district(&room.info.district);
                let era = era_name_from_z(room.info.z);
                format!(" {} \u{2014} {} | {} | [{},{},{}] ",
                    room_name, dist, era, coord[0], coord[1], coord[2])
            } else {
                format!(" The Stacks [{},{},{}] ", coord[0], coord[1], coord[2])
            };
            let title_block = Block::default()
                .borders(Borders::ALL)
                .title(title);

            let inner = title_block.inner(chunks[0]);
            frame.render_widget(title_block, chunks[0]);

            let player_pos = app.engine.player.coord();
            let mut actors: Vec<(hex2d_stacks::Coordinate<i32>, ActorKind)> = Vec::new();
            // Librarians first, then scribes, then player on top
            for lib in &app.engine.librarians {
                actors.push((lib.coord(), ActorKind::Librarian));
            }
            for scribe in &app.engine.scribes {
                actors.push((scribe.coord(), ActorKind::Scribe));
            }
            actors.push((player_pos, ActorKind::Player));

            let room_view = RoomView {
                tiles: &app.world.tiles,
                actors: &actors,
                player_pos,
                palette: app.current_palette,
                rooms: &app.world.rooms,
                light_sources: &light_sources,
                tick: app.tick,
                room_particles: app.room_particles.get(&app.engine.current_room),
                ritual_ring: app.ritual_ring.as_ref(),
                gather_vortices: &app.gather_vortices,
            };
            frame.render_widget(room_view, inner);

            // Compass: always on, all modes, all popups
            render::hud::render_compass_always(
                &app.world, &app.engine, app.current_palette,
                inner, frame.buffer_mut(),
            );

            // Gather toasts: top-left, always visible
            if !app.gather_toasts.is_empty() {
                render::hud::render_gather_toasts(
                    &app.gather_toasts, inner, frame.buffer_mut(),
                );
            }

            // HUD overlay (between room view and popups)
            let hud_hidden_by_popup = match &app.active_popup {
                Some(PopupContent::Dialog { .. }) => false,
                Some(PopupContent::ScribeChat { .. }) => false,
                Some(PopupContent::PauseMenu { .. }) => false,
                Some(PopupContent::Chat { .. }) => true,
                Some(_) => true,
                None => false,
            };
            if app.hud_mode != HudMode::Off && !hud_hidden_by_popup {
                let hud_data = if let Some(room) = room_meta {
                    HudData {
                        room_name: room_name_from_cluster(
                            &room.info.cluster_id,
                            &room.info.cluster_signature,
                            &room.info.room_type,
                        ),
                        district: display_district(&room.info.district).to_string(),
                        era: era_name_from_z(room.info.z).to_string(),
                    }
                } else {
                    HudData {
                        room_name: "Unknown".to_string(),
                        district: "???".to_string(),
                        era: "???".to_string(),
                    }
                };
                render_hud(
                    &app.world, &app.engine, &app.breadcrumb_trail,
                    app.current_palette, &hud_data, app.hud_mode,
                    inner, frame.buffer_mut(),
                );
            }

            if let Some(ref popup) = app.active_popup {
                let chat_title = if matches!(popup, PopupContent::Chat { .. }) {
                    let model = if app.hermes_model.is_empty() {
                        "connecting..."
                    } else {
                        &app.hermes_model
                    };
                    let ctx_used = app.token_budget.context_used / 1024;
                    let ctx_max = app.token_budget.context_window / 1024;
                    Some(format!(
                        " {} \u{2014} {} \u{2014} {}K/{}K ",
                        model, app.hermes_endpoint, ctx_used, ctx_max
                    ))
                } else {
                    None
                };
                render_popup(popup, chat_title.as_deref(), inner, frame.buffer_mut());

                // Fullscreen map: render hex content after popup draws the border
                if let PopupContent::PauseMenu { view: render::popup::PauseView::Map, map_center, .. } = popup {
                    let map_inner = Rect::new(
                        inner.x + 1, inner.y + 1,
                        inner.width.saturating_sub(2), inner.height.saturating_sub(2),
                    );
                    render::hud::minimap::render_fullscreen_map(
                        &app.world, *map_center, app.engine.current_room,
                        map_inner, frame.buffer_mut(),
                    );
                }
            }

            // Chat sidebar (minimized, read-only) — shown when hud_mode is ChatSidebar
            // and no modal popup is open
            if app.hud_mode == HudMode::ChatSidebar && app.active_popup.is_none() {
                if let Some(ref cs) = app.chat_state {
                    render_chat_sidebar(
                        &cs.messages,
                        &app.sidebar_streaming,
                        &app.sidebar_status,
                        &app.sidebar_status_text,
                        inner, frame.buffer_mut(),
                    );
                }
            }

        })?;

        // When recording (STACKS_FULL_REDRAW), poison the comparison buffer so
        // ratatui diffs every cell as changed next frame — including "default"
        // cells that popups leave as background. Without this, ratatui's diff
        // skips unchanged cells and asciinema/agg loses them on chunk boundaries.
        //
        // After draw(): current points to the empty buffer (will be rendered into
        // next); buffers[1-current] holds the just-rendered frame (comparison
        // target for next flush). We swap to reach it, fill with sentinel, swap back.
        if full_redraw {
            // swap 1: now current points to the just-rendered frame
            terminal.swap_buffers();
            // poison every cell with a sentinel no widget produces
            let buf = terminal.current_buffer_mut();
            for cell in buf.content.iter_mut() {
                cell.set_char('\x01');
            }
            // swap 2: restores original orientation; the poisoned buffer
            // is now at 1-current — exactly where flush() reads from
            terminal.swap_buffers();
        }

        // Agent state dump — write structured JSON sidecar for external agent
        let hud_str = match app.hud_mode {
            HudMode::Off => "Off",
            HudMode::Standard => "Standard",
            HudMode::ChatSidebar => "ChatSidebar",
        };
        game::agent_state::write_agent_state(
            &app.engine, &app.world, app.popup_name(), hud_str,
        );
        // Agent pathfinding: check for path request, compute A*, write response
        game::agent_state::check_path_request(&app.engine, &app.world);

        // Drain background preload results (non-blocking)
        app.drain_preload_results();

        // Drain background search results (non-blocking)
        if let Some(ref handle) = app.search_pending {
            if handle.is_finished() {
                let handle = app.search_pending.take().unwrap();
                if let Ok(results) = handle.await {
                    if let Some(PopupContent::PauseMenu { ref mut search_results, ref mut search_loading, ref mut search_selected, .. }) = app.active_popup {
                        *search_results = results;
                        *search_loading = false;
                        *search_selected = 0;
                    }
                }
            }
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code).await;
                }
            }
        }

        // Check for OS signals (SIGTERM, SIGHUP)
        if quit_signal.load(Ordering::Relaxed) {
            app.should_quit = true;
        }

        if app.should_quit {
            // Paint farewell screen before exiting
            terminal.clear()?;
            terminal.draw(|frame| {
                let area = frame.area();
                let gold = Color::Rgb(196, 146, 42);
                let dim_gold = Color::Rgb(130, 97, 28);
                let dark = Color::Rgb(60, 50, 35);
                let stone = Color::Rgb(90, 80, 65);

                let lines = vec![
                    Line::from(""),
                    Line::from(""),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    \u{2727}",
                        Style::default().fg(gold),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    wp r\u{0334}\u{0323} \u{2014} The Closing of the Mouth",
                        Style::default().fg(gold),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    The words have been spoken.",
                        Style::default().fg(stone),
                    )),
                    Line::from(Span::styled(
                        "    The scrolls rest upon their shelves.",
                        Style::default().fg(stone),
                    )),
                    Line::from(Span::styled(
                        "    The Librarian returns to silence.",
                        Style::default().fg(stone),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    May your ka traverse the Duat in peace,",
                        Style::default().fg(dim_gold),
                    )),
                    Line::from(Span::styled(
                        "    and your name endure upon the stone.",
                        Style::default().fg(dim_gold),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    The Stacks endure.",
                        Style::default().fg(gold),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    \u{2727}",
                        Style::default().fg(gold),
                    )),
                    Line::from(""),
                    Line::from(""),
                    Line::from(Span::styled(
                        "    ii m Htp",
                        Style::default().fg(dark),
                    )),
                ];

                let para = Paragraph::new(lines);
                frame.render_widget(para, area);
            })?;
            // Hold the farewell briefly so it's visible
            std::thread::sleep(std::time::Duration::from_millis(5400));
            // Clear to blank before leaving
            terminal.clear()?;
            terminal.draw(|_frame| {})?;
            break;
        }
    }

    // Clean up agent state file + command socket
    game::agent_state::remove_agent_state();
    game::command_server::cleanup();

    // Shutdown bridges
    if let Some(handle) = app.bridge_pending.take() {
        handle.abort();
    }
    if let Some(mut bridge) = app.hermes_bridge.take() {
        bridge.close().await;
    }
    if let Some(mut bridge) = app.scribe_bridge.take() {
        bridge.kill().await;
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    Ok(())
}
