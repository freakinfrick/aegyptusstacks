use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap, Clear};

use crate::game::hermes::{ChatMessage, ChatRole, ChatStatus, SessionSummary};
use crate::game::scribe::ScribeStatus;

/// Map a physical_type string to its corpus fixture (glyph, Color).
/// Returns the tile's glyph symbol and its vibrant fixture color.
fn fixture_style(ptype: &str) -> (&'static str, Color) {
    let s = ptype.to_lowercase();
    if s.contains("papyrus") || s.contains("writing") {
        ("││", Color::Rgb(240, 200, 80))      // papyrus — vivid amber
    } else if s.contains("stele") || s.contains("stel") {
        ("▐▌", Color::Rgb(200, 195, 185))     // stele — bright silver
    } else if s.contains("scroll") || s.contains("schriftrolle") {
        ("()", Color::Rgb(235, 225, 180))      // scroll — warm cream
    } else if s.contains("vessel") || s.contains("object") || s.contains("gef") {
        ("♦♦", Color::Rgb(230, 140, 65))      // vessel — bright terracotta
    } else if s.contains("coffin") || s.contains("sarg") {
        ("⊏⊐", Color::Rgb(150, 80, 200))     // coffin — vivid purple
    } else if s.contains("sculpt") || s.contains("skulptur") {
        ("☥☥", Color::Rgb(60, 210, 185))      // sculpture — faience turquoise
    } else if s.contains("ostra") {
        (",,", Color::Rgb(220, 190, 90))       // ostrakon — warm ochre
    } else if s.contains("wall") || s.contains("inscription") || s.contains("wand") {
        ("══", Color::Rgb(255, 220, 60))       // wall inscription — blazing gold
    } else if s.contains("false") || s.contains("scheint") || s.contains("doorway") {
        ("╔╗", Color::Rgb(220, 75, 60))        // false door — vivid crimson
    } else if s.contains("tomb") || s.contains("grab") {
        ("▼▼", Color::Rgb(140, 80, 180))      // tomb alcove — rich violet
    } else if s.contains("rock") || s.contains("fels") {
        ("▓▓", Color::Rgb(195, 120, 70))      // rock face — warm sienna
    } else {
        ("≡≡", Color::Rgb(200, 170, 120))     // generic shelf — warm neutral
    }
}

/// Dim a color to ~40% brightness for unselected items.
fn dim_color(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as u16 * 40 / 100) as u8,
            (g as u16 * 40 / 100) as u8,
            (b as u16 * 40 / 100) as u8,
        ),
        other => other,
    }
}

/// Which sub-view the pause menu is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauseView {
    /// Main menu (Resume / Controls / Sessions / Room Lookup / Quit)
    Menu,
    /// Keybinding reference
    Controls,
    /// Browse and resume prior librarian sessions
    Sessions,
    /// Room coordinate lookup — enter coords, see result
    RoomLookup,
    /// Corpus text search
    CorpusSearch,
    /// Article reader — browse and read scribe articles
    Articles,
    /// Inventory browser — rich view of gathered texts
    Inventory,
    /// Fullscreen scrollable map
    Map,
}

/// An article entry for the article reader pause menu.
#[derive(Clone, Debug)]
pub struct ArticleEntry {
    pub filename: String,
    pub title: String,
    pub word_count: u32,
    pub date: String,
}

/// A single corpus search result for the pause menu search UI.
#[derive(Clone, Debug)]
pub struct CorpusSearchHit {
    pub canonical_id: String,
    pub name: String,
    pub snippet: String,
    pub coord: Option<[i32; 3]>,
    pub district: String,
    pub z_band: Option<i32>,
}

/// Result of a BFS room lookup from the pause menu.
#[derive(Clone, Debug)]
pub struct RoomLookupResult {
    pub coord: [i32; 3],
    pub name: String,
    pub district: String,
    pub era: String,
    pub volume_count: u32,
    pub composition: String,
    pub cluster_sig: Vec<String>,
    pub error: Option<String>,
}

/// Content to display in a popup
#[derive(Clone, Debug)]
pub enum PopupContent {
    ShelfInspect {
        title: String,
        volumes: Vec<String>,
    },
    /// Interactive shelf menu — player selects a text to read
    ShelfMenu {
        title: String,
        /// (canonical_id, display_name, physical_type) triples
        entries: Vec<(String, String, String)>,
        selected: usize,
    },
    LecternRead {
        title: String,
        sentences: Vec<(Option<String>, Option<String>)>,  // (transliteration/source, translation/body)
        scroll_offset: usize,
    },
    /// Simple text dialog (librarian interaction, future chat)
    Dialog {
        title: String,
        text: String,
    },
    /// Scribe writing overlay — read-only during synthesis, scrollable after.
    ScribeChat {
        title: String,
        streaming_text: String,
        status: ScribeStatus,
        status_text: Option<String>,
        scroll_offset: usize,
        word_count: u32,
        model: String,
        volume_count: u32,
        passage_count: u32,
        reasoning_ring: Vec<String>,
    },
    /// Pause menu — Resume / Controls / Room Lookup / Quit
    PauseMenu {
        selected: usize,
        view: PauseView,
        /// Coordinate input buffer for room lookup ("q,r,z")
        coord_input: String,
        /// Last room lookup result
        lookup_result: Option<RoomLookupResult>,
        /// Player inventory snapshot: Vec<(canonical_id, room_coord, name, physical_type, district)>
        inventory_items: Vec<(String, [i32; 3], String, String, String)>,
        /// Selected index within inventory view
        inventory_selected: usize,
        /// Prior sessions list (populated on demand)
        session_list: Vec<SessionSummary>,
        /// Selected index within session list
        session_selected: usize,
        /// Loading state for async session.list fetch
        sessions_loading: bool,
        /// Map camera center (BFS q, r) and z-level
        map_center: [i32; 3],
        /// Corpus search input buffer
        search_input: String,
        /// Corpus search results
        search_results: Vec<CorpusSearchHit>,
        /// Selected index within search results
        search_selected: usize,
        /// Search loading state
        search_loading: bool,
        /// Article list for the article reader
        article_list: Vec<ArticleEntry>,
        /// Selected article index
        article_selected: usize,
        /// Article body text (loaded on selection)
        article_body: String,
        /// Scroll offset within loaded article
        article_scroll: usize,
    },
    /// Interactive chat session with the Librarian (hermes-local)
    Chat {
        messages: Vec<ChatMessage>,
        input_buffer: String,
        input_cursor: usize,
        streaming_partial: String,
        /// True when streaming_partial contains thinking/reasoning (vs content).
        streaming_is_thinking: bool,
        status: ChatStatus,
        scroll_offset: usize,
        status_text: Option<String>,
    },
    /// Article reader — opened from ArticleDisplay tile
    ArticleRead {
        title: String,
        body: String,
        scroll_offset: usize,
    },
}

/// Render a centered popup overlay.
/// Source/metadata in mid-gray, body text in white, titles in gold.
/// `chat_title` is an optional pre-formatted title for the Chat popup border.
pub fn render_popup(content: &PopupContent, chat_title: Option<&str>, area: Rect, buf: &mut Buffer) {
    let gold = Color::Rgb(196, 146, 42);
    let mid_gray = Color::Rgb(140, 140, 140);
    let white = Color::Rgb(240, 235, 220);

    // Full-size popup area (used by shelf/lectern popups, not Dialog)
    let popup_width = (area.width as f32 * 0.70) as u16;
    let popup_height = (area.height as f32 * 0.80) as u16;
    let popup_x = area.x + (area.width - popup_width) / 2;
    let popup_y = area.y + (area.height - popup_height) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let black = Color::Rgb(0, 0, 0);

    match content {
        PopupContent::ShelfInspect { title, volumes } => {
            Clear.render(popup_area, buf);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    format!(" {} ", title),
                    Style::default().fg(black).bg(gold),
                ))
                .border_style(Style::default().fg(gold));

            let mut lines: Vec<Line> = Vec::new();
            if volumes.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  (empty shelf)", Style::default().fg(mid_gray),
                )));
            } else {
                lines.push(Line::from("")); // top padding
                for (i, v) in volumes.iter().enumerate() {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {}. ", i + 1), Style::default().fg(mid_gray)),
                        Span::styled(v.as_str(), Style::default().fg(white)),
                    ]));
                }
            }

            let paragraph = Paragraph::new(lines)
                .block(block);
            paragraph.render(popup_area, buf);
        }
        PopupContent::ShelfMenu { title, entries, selected } => {
            Clear.render(popup_area, buf);

            // Derive border color from the first entry's fixture type
            let border_color = entries.first()
                .map(|(_, _, pt)| fixture_style(pt).1)
                .unwrap_or(gold);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(format!(" {} ", title), Style::default().fg(black).bg(border_color)),
                    ).alignment(Alignment::Left)
                )
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" W/X  Space  Esc ", Style::default().fg(Color::Rgb(90, 85, 75)))
                    ).alignment(Alignment::Right)
                )
                .border_style(Style::default().fg(border_color));

            let inner_w = popup_width.saturating_sub(2) as usize; // content width inside borders
            let dim_white = Color::Rgb(160, 155, 145);
            let dark = Color::Rgb(80, 75, 65);

            let inner_height = popup_height.saturating_sub(2) as usize;

            // Build all entry lines first, tracking which line each entry starts on
            let mut all_lines: Vec<Line> = Vec::new();
            let mut entry_start_lines: Vec<usize> = Vec::new();

            all_lines.push(Line::from("")); // top padding

            for (i, (id, name, ptype)) in entries.iter().enumerate() {
                entry_start_lines.push(all_lines.len());
                let is_sel = i == *selected;
                let (glyph, fixture_color) = fixture_style(ptype);

                let glyph_color = if is_sel { fixture_color } else { dim_color(fixture_color) };
                let name_color = if is_sel { white } else { dim_white };
                let tag_color = if is_sel { fixture_color } else { dark };
                let pointer = if is_sel { "\u{25b8}" } else { " " };
                let pointer_color = if is_sel { fixture_color } else { dark };

                // Type tag as prefix — capitalize first letter
                let type_tag = if ptype.is_empty() {
                    String::new()
                } else {
                    let mut c = ptype.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => format!("{}{}", f.to_uppercase().collect::<String>(), c.as_str()),
                    }
                };

                // Layout: "  ▸ () Tag  Name..."
                // prefix_len: 2(pad) + 1(ptr) + 1(sp) + 2(glyph) + 1(sp) + tag.len + 2(sp before name)
                let prefix_len = 2 + 1 + 1 + 2 + 1 + type_tag.len() + 2;
                let name_width = inner_w.saturating_sub(prefix_len);

                // Word-wrap name into lines, first line gets prefix, continuations get indent
                let name_lines = wrap_to_width(name, name_width);

                // Show canonical ID dim after name on first line
                let id_suffix = format!(" \u{00b7} {}", id);
                let id_color = if is_sel { Color::Rgb(120, 115, 105) } else { Color::Rgb(70, 65, 55) };

                for (li, name_chunk) in name_lines.into_iter().enumerate() {
                    if li == 0 {
                        let mut spans = vec![
                            Span::styled(format!("  {} ", pointer), Style::default().fg(pointer_color)),
                            Span::styled(format!("{} ", glyph), Style::default().fg(glyph_color)),
                        ];
                        if !type_tag.is_empty() {
                            spans.push(Span::styled(format!("{}  ", type_tag), Style::default().fg(tag_color)));
                        }
                        spans.push(Span::styled(name_chunk, Style::default().fg(name_color)));
                        spans.push(Span::styled(id_suffix.clone(), Style::default().fg(id_color)));
                        all_lines.push(Line::from(spans));
                    } else {
                        let pad = " ".repeat(prefix_len);
                        all_lines.push(Line::from(
                            Span::styled(format!("{}{}", pad, name_chunk), Style::default().fg(name_color)),
                        ));
                    }
                }

                // Visual grouping separator every 5 entries
                if i + 1 < entries.len() && (i + 1) % 5 == 0 {
                    all_lines.push(Line::from(""));
                }
            }

            // Scroll: ensure the selected entry is visible within inner_height
            let sel_line = entry_start_lines.get(*selected).copied().unwrap_or(0);
            let total = all_lines.len();
            let skip = if total <= inner_height {
                0
            } else {
                // Keep selected entry roughly centered, clamped to bounds
                let half = inner_height / 2;
                sel_line.saturating_sub(half).min(total.saturating_sub(inner_height))
            };
            let visible: Vec<Line> = all_lines.into_iter().skip(skip).take(inner_height).collect();

            let paragraph = Paragraph::new(visible)
                .block(block);
            paragraph.render(popup_area, buf);
        }
        PopupContent::Dialog { title, text } => {
            let magenta = Color::Magenta;

            // Left-anchored, bottom-justified persistent chat panel
            let dlg_width = 40u16.min(area.width);
            let dlg_height = (area.height * 3 / 4).max(8).min(area.height);
            let dlg_x = area.x + 1;
            let dlg_y = area.y + area.height.saturating_sub(dlg_height); // bottom-justified
            let dlg_area = Rect::new(dlg_x, dlg_y, dlg_width, dlg_height);

            Clear.render(dlg_area, buf);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    format!(" {} ", title),
                    Style::default().fg(black).bg(magenta),
                ))
                .title_position(ratatui::widgets::block::Position::Top)
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" [esc] ", Style::default().fg(mid_gray))
                    ).alignment(Alignment::Right)
                )
                .border_style(Style::default().fg(magenta));

            // Content: text at top, rest is empty space (future chat messages)
            let inner_height = dlg_height.saturating_sub(2) as usize;
            let mut lines: Vec<Line> = Vec::new();

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", text),
                Style::default().fg(white),
            )));

            // Fill remaining space
            while lines.len() < inner_height {
                lines.push(Line::from(""));
            }

            let paragraph = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false });
            paragraph.render(dlg_area, buf);
        }
        PopupContent::LecternRead { title, sentences, scroll_offset } => {
            Clear.render(popup_area, buf);

            // Detect fixture type from the second sentence (type header line):
            // format is "Stele: 2/5 — TM12345" or "Papyrus: 1/3 — ..."
            let header_type = sentences.get(1)
                .and_then(|(src, _)| src.as_deref())
                .unwrap_or("");
            let (title_glyph, title_fixture_color) = fixture_style(header_type);

            let block = Block::default()
                .borders(Borders::ALL)
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(
                            format!(" {} {} ", title_glyph, title),
                            Style::default().fg(black).bg(title_fixture_color),
                        ),
                    ).alignment(Alignment::Left)
                )
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" W/X scroll  \u{2190}/\u{2192} flip  G gather  Esc back ", Style::default().fg(Color::Rgb(90, 85, 75)))
                    ).alignment(Alignment::Right)
                )
                .border_style(Style::default().fg(title_fixture_color));

            let inner_height = popup_height.saturating_sub(2) as usize;
            let inner_w = popup_width.saturating_sub(2) as usize;
            let text_w = inner_w.saturating_sub(4); // 2-char left pad + 2 margin
            let body_w = inner_w.saturating_sub(6); // 4-char indent for translations
            let mut lines: Vec<Line> = Vec::new();
            let transliteration_color = Color::Rgb(140, 135, 125);

            for (translit, trans) in sentences.iter().skip(*scroll_offset) {
                if lines.len() >= inner_height {
                    break;
                }

                match (translit.as_deref(), trans.as_deref()) {
                    // Header line — source only: could be title or type metadata
                    (Some(src), None) => {
                        if src.contains(':') && (src.contains('/') || src.contains("Text ")) {
                            let (glyph, color) = fixture_style(src);
                            lines.push(Line::from(vec![
                                Span::styled(format!("  {} ", glyph), Style::default().fg(color)),
                                Span::styled(src, Style::default().fg(color)),
                            ]));
                        } else {
                            for wl in wrap_to_width(src, text_w) {
                                lines.push(Line::from(Span::styled(
                                    format!("  {}", wl),
                                    Style::default().fg(white),
                                )));
                            }
                        }
                    }
                    (None, Some(body)) => {
                        if body.starts_with("---") {
                            lines.push(Line::from(Span::styled(
                                format!("  {}", body),
                                Style::default().fg(Color::Rgb(70, 65, 60)),
                            )));
                        } else {
                            for wl in wrap_to_width(body, body_w) {
                                lines.push(Line::from(Span::styled(
                                    format!("    {}", wl),
                                    Style::default().fg(white),
                                )));
                            }
                        }
                    }
                    (Some(src), Some(body)) => {
                        for wl in wrap_to_width(src, text_w) {
                            lines.push(Line::from(Span::styled(
                                format!("  {}", wl),
                                Style::default().fg(transliteration_color),
                            )));
                        }
                        for wl in wrap_to_width(body, body_w) {
                            lines.push(Line::from(Span::styled(
                                format!("    {}", wl),
                                Style::default().fg(white),
                            )));
                        }
                        lines.push(Line::from(""));
                    }
                    (None, None) => {
                        lines.push(Line::from(""));
                    }
                }
            }

            let paragraph = Paragraph::new(lines)
                .block(block);
            paragraph.render(popup_area, buf);
        }
        PopupContent::ScribeChat {
            title: _,
            streaming_text,
            status,
            status_text,
            scroll_offset,
            word_count,
            model,
            volume_count,
            passage_count,
            reasoning_ring,
        } => {
            let scribe_cyan = Color::Rgb(0, 210, 210);
            let dim_cyan = Color::Rgb(0, 130, 130);
            let dark_cyan = Color::Rgb(0, 80, 80);
            let warm_white = Color::Rgb(240, 235, 220);

            Clear.render(popup_area, buf);

            // Top title: " The Scribe " + status
            let status_indicator = match status {
                ScribeStatus::Idle => "",
                ScribeStatus::Fetching => " fetching...",
                ScribeStatus::Reasoning => " reasoning...",
                ScribeStatus::Writing => " writing...",
                ScribeStatus::Complete => " complete",
                ScribeStatus::Error(_) => " error",
            };

            // Bottom title: model + volume metadata
            let model_short = if model.contains('/') {
                model.split('/').last().unwrap_or(model)
            } else if model.is_empty() {
                "kimi-k2-thinking"
            } else {
                model
            };
            let meta_str = if *volume_count > 0 {
                format!(" {} \u{00b7} {} vols \u{00b7} {} passages ", model_short, volume_count, passage_count)
            } else {
                format!(" {} ", model_short)
            };

            let block = Block::default()
                .borders(Borders::ALL)
                .title(
                    ratatui::widgets::block::Title::from(vec![
                        Span::styled(" The Scribe ", Style::default().fg(black).bg(scribe_cyan)),
                        Span::styled(
                            &meta_str,
                            Style::default().fg(black).bg(dim_cyan),
                        ),
                        Span::styled(status_indicator, Style::default().fg(dim_cyan)),
                        Span::raw(" "),
                    ]).alignment(Alignment::Left)
                )
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" [esc] ", Style::default().fg(mid_gray))
                    ).alignment(Alignment::Right)
                )
                .border_style(Style::default().fg(scribe_cyan));

            let inner_width = popup_width.saturating_sub(2) as usize;
            let pad_width = inner_width.saturating_sub(4);
            let inner_height = popup_height.saturating_sub(2) as usize;

            // Status / progress line at top
            let mut all_lines: Vec<Line> = Vec::new();
            let status_line = if let Some(st) = status_text {
                format!("  \u{270d} {}", st)
            } else {
                match status {
                    ScribeStatus::Reasoning => "  \u{2726} Model is reasoning...".to_string(),
                    ScribeStatus::Writing => format!("  \u{270d} Writing... ({} words)", word_count),
                    ScribeStatus::Complete => format!("  \u{2713} Complete \u{2014} {} words \u{2014} saved", word_count),
                    ScribeStatus::Fetching => "  \u{270d} Fetching volumes from the library...".to_string(),
                    ScribeStatus::Error(ref e) => format!("  \u{2717} {}", e),
                    ScribeStatus::Idle => "  Waiting...".to_string(),
                }
            };
            all_lines.push(Line::from(Span::styled(status_line, Style::default().fg(dim_cyan))));
            all_lines.push(Line::from(""));

            // Body: reasoning + article in one continuous stream
            let mut body_lines: Vec<Line> = Vec::new();
            let body_area = inner_height.saturating_sub(2);

            // ── Reasoning text (grey/dim, always shown first) ──
            if !reasoning_ring.is_empty() {
                let reasoning_style = Style::default().fg(dark_cyan);
                for rline in reasoning_ring.iter() {
                    if rline.trim().is_empty() {
                        body_lines.push(Line::from(""));
                        continue;
                    }
                    let padded = format!("  {}", rline);
                    let wrapped = wrap_text(&padded, pad_width);
                    for wline in wrapped {
                        body_lines.push(Line::from(Span::styled(wline, reasoning_style)));
                    }
                }
            }

            // ── Article text (bright, follows reasoning) ──
            if !streaming_text.is_empty() {
                // Separator between reasoning and article
                if !reasoning_ring.is_empty() {
                    body_lines.push(Line::from(""));
                }
                for para in streaming_text.split('\n') {
                    if para.trim().is_empty() {
                        body_lines.push(Line::from(""));
                        continue;
                    }
                    let trimmed = para.trim();
                    let (prefix, color) = if trimmed.starts_with('>') {
                        ("  ", Color::Rgb(180, 230, 230))  // bright cyan-white for quotes
                    } else if trimmed.starts_with('#') {
                        ("  ", scribe_cyan)  // headings in cyan
                    } else if trimmed.starts_with("---") || trimmed.starts_with("***") {
                        ("  ", dim_cyan)
                    } else {
                        ("  ", warm_white)
                    };
                    let padded = format!("{}{}", prefix, para);
                    let wrapped = wrap_text(&padded, pad_width);
                    for wline in wrapped {
                        body_lines.push(Line::from(Span::styled(wline, Style::default().fg(color))));
                    }
                }
            }

            // Unified scroll logic — auto-scroll to bottom
            let total = body_lines.len();
            let auto_bottom = total.saturating_sub(body_area);
            let skip = if *scroll_offset > 0 {
                auto_bottom.saturating_sub(*scroll_offset)
            } else {
                auto_bottom
            };
            let visible: Vec<Line> = body_lines.into_iter().skip(skip).take(body_area).collect();
            for line in visible {
                all_lines.push(line);
            }

            while all_lines.len() < inner_height {
                all_lines.push(Line::from(""));
            }

            let paragraph = Paragraph::new(all_lines)
                .block(block)
                .wrap(Wrap { trim: false });
            paragraph.render(popup_area, buf);
        }
        PopupContent::PauseMenu { selected, view, coord_input, lookup_result, inventory_items, inventory_selected, session_list, session_selected, sessions_loading, map_center, search_input, search_results, search_selected, search_loading, article_list, article_selected, article_body, article_scroll } => {
            let accent = Color::Rgb(196, 146, 42); // gold
            let dim_gold = Color::Rgb(130, 97, 28);
            let warm_white = Color::Rgb(240, 235, 220);
            let dark = Color::Rgb(80, 75, 65);

            // ── Fullscreen map: takes over entire area ──
            if *view == PauseView::Map {
                // Need access to world — passed via chat_title hack (world pointer not available here).
                // Instead, render just the frame; actual hex content is rendered by caller.
                // We render the border + status here.
                Clear.render(area, buf);

                let z = map_center[2];
                let title_str = format!(
                    " Map \u{2014} Z:{} \u{2014} [{},{}] ",
                    z, map_center[0], map_center[1]
                );
                let hint_str = " QWEDCXZA pan | R/F level | Esc back ";

                let border_style = Style::default().fg(accent);
                // Top border
                buf.set_string(area.x, area.y, "\u{250c}", border_style);
                buf.set_string(area.x + 1, area.y, &title_str, Style::default().fg(black).bg(accent));
                let title_end = area.x + 1 + title_str.len() as u16;
                for x in title_end..area.right().saturating_sub(1) {
                    buf.set_string(x, area.y, "\u{2500}", border_style);
                }
                buf.set_string(area.right().saturating_sub(1), area.y, "\u{2510}", border_style);

                // Side borders
                for y in (area.y + 1)..area.bottom().saturating_sub(1) {
                    buf.set_string(area.x, y, "\u{2502}", border_style);
                    buf.set_string(area.right().saturating_sub(1), y, "\u{2502}", border_style);
                }

                // Bottom border with hint
                buf.set_string(area.x, area.bottom().saturating_sub(1), "\u{2514}", border_style);
                for x in (area.x + 1)..area.right().saturating_sub(1) {
                    buf.set_string(x, area.bottom().saturating_sub(1), "\u{2500}", border_style);
                }
                buf.set_string(area.right().saturating_sub(1), area.bottom().saturating_sub(1), "\u{2518}", border_style);
                // Hint right-aligned in bottom border
                let hint_x = area.right().saturating_sub(hint_str.len() as u16 + 1);
                buf.set_string(hint_x, area.bottom().saturating_sub(1), hint_str, Style::default().fg(dark));

                // The actual hex map content is rendered by the caller (main.rs)
                // after this popup renders, since it needs WorldMap access.
                // We leave the inner area clear for it.
                return;
            }

            // ���─ Left panel: menu items (always visible) ──
            let menu_width = 28u16.min(area.width / 3);
            let menu_height = (area.height * 3 / 4).max(15).min(area.height.saturating_sub(2));
            let menu_x = area.x + 2;
            let menu_y = area.y + (area.height.saturating_sub(menu_height)) / 2;
            let menu_area = Rect::new(menu_x, menu_y, menu_width, menu_height);

            Clear.render(menu_area, buf);

            let menu_block = Block::default()
                .borders(Borders::ALL)
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" The Stacks ", Style::default().fg(black).bg(accent))
                    ).alignment(Alignment::Center)
                )
                .border_style(Style::default().fg(if *view == PauseView::Menu { accent } else { dim_gold }));

            let items = ["Resume", "Controls", "Map", "Sessions", "Corpus Search", "Articles", "Room Lookup", "Quit"];
            let menu_item_count = items.len();
            let mut menu_lines: Vec<Line> = Vec::new();
            menu_lines.push(Line::from("")); // top padding
            menu_lines.push(Line::from(Span::styled(
                "     PAUSED",
                Style::default().fg(dim_gold),
            )));
            menu_lines.push(Line::from("")); // spacing

            for (i, label) in items.iter().enumerate() {
                let is_sel = *view == PauseView::Menu && i == *selected;
                let pointer = if is_sel { "\u{25b8} " } else { "  " };
                let color = if is_sel { accent } else { dark };
                let text_color = if is_sel { warm_white } else { mid_gray };

                menu_lines.push(Line::from(vec![
                    Span::styled(format!("    {}", pointer), Style::default().fg(color)),
                    Span::styled(*label, Style::default().fg(text_color)),
                ]));
                if i + 1 < menu_item_count {
                    menu_lines.push(Line::from("")); // spacing between items
                }
            }

            // Inventory summary at bottom of menu panel
            let inner_h = menu_height.saturating_sub(2) as usize;
            while menu_lines.len() < inner_h.saturating_sub(2) {
                menu_lines.push(Line::from(""));
            }
            let inv_count = inventory_items.len();
            if inv_count > 0 {
                menu_lines.push(Line::from(Span::styled(
                    format!("  \u{27d0} {} gathered", inv_count),
                    Style::default().fg(accent),
                )));
            } else {
                menu_lines.push(Line::from(Span::styled(
                    "  inventory empty",
                    Style::default().fg(dark),
                )));
            }
            menu_lines.push(Line::from(Span::styled(
                "  Esc resume",
                Style::default().fg(Color::Rgb(60, 55, 50)),
            )));

            let menu_para = Paragraph::new(menu_lines).block(menu_block);
            menu_para.render(menu_area, buf);

            // ── Right panel: context-dependent ──
            let right_x = menu_x + menu_width;
            let right_width = area.width.saturating_sub(right_x - area.x).saturating_sub(3);
            if right_width < 20 { return; } // terminal too narrow
            let right_area = Rect::new(right_x, menu_y, right_width, menu_height);

            Clear.render(right_area, buf);

            match view {
                PauseView::Menu | PauseView::Inventory => {
                    // ── Inventory detail (shared between menu summary + full view) ──
                    let is_full = matches!(view, PauseView::Inventory);
                    let hint = if is_full { " \u{2191}\u{2193} select  Space read  Esc back " } else { " \u{2192} browse inventory " };
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(
                                    format!(" Inventory \u{2014} {} texts ", inv_count),
                                    Style::default().fg(black).bg(accent),
                                )
                            ).alignment(Alignment::Left)
                        )
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(hint, Style::default().fg(dark))
                            ).alignment(Alignment::Right).position(ratatui::widgets::block::Position::Bottom)
                        )
                        .border_style(Style::default().fg(dim_gold));

                    let r_inner_h = menu_height.saturating_sub(2) as usize;
                    let mut lines: Vec<Line> = Vec::new();

                    if inventory_items.is_empty() {
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "  No texts gathered yet.",
                            Style::default().fg(dark),
                        )));
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "  Use G at a shelf to gather",
                            Style::default().fg(Color::Rgb(60, 55, 50)),
                        )));
                        lines.push(Line::from(Span::styled(
                            "  texts into your collection.",
                            Style::default().fg(Color::Rgb(60, 55, 50)),
                        )));
                    } else {
                        use super::hud::glyph_for_physical_type;

                        // Scroll to keep selection visible
                        let scroll_start = if is_full {
                            let vis = r_inner_h.saturating_sub(1);
                            if *inventory_selected >= vis { inventory_selected.saturating_sub(vis - 1) } else { 0 }
                        } else { 0 };

                        for (i, (id, room, name, ptype, district)) in inventory_items.iter().enumerate().skip(scroll_start) {
                            if lines.len() + 2 >= r_inner_h { break; }
                            let (glyph, glyph_color) = glyph_for_physical_type(ptype);
                            let pointer = if is_full && i == *inventory_selected { "\u{25b8} " } else { "  " };
                            let display_name = if name.is_empty() { id.as_str() } else { name.as_str() };

                            // Type tag
                            let type_tag = match ptype.as_str() {
                                "papyrus" => "Papyrus",
                                "stele" | "rounded_stele" | "rectangular_stele" => "Stele",
                                "scroll" => "Scroll",
                                "vessel" | "object" => "Vessel",
                                "coffin" => "Coffin",
                                "wall_inscription" | "wall" => "Inscription",
                                "ostrakon" | "ostracon" => "Ostrakon",
                                "tomb" => "Tomb",
                                "sculpture" => "Sculpture",
                                "false_door" | "doorway" => "False Door",
                                "rock" | "rock_cut" => "Rock",
                                _ => "Text",
                            };

                            // Name line: glyph + type + name
                            let inner_w = right_width.saturating_sub(4) as usize;
                            let max_name = inner_w.saturating_sub(pointer.len() + 2 + type_tag.len() + 3);
                            let trunc_name = if display_name.len() > max_name && max_name > 3 {
                                format!("{}...", &display_name[..display_name.floor_char_boundary(max_name - 3)])
                            } else {
                                display_name.to_string()
                            };

                            lines.push(Line::from(vec![
                                Span::styled(pointer, Style::default().fg(accent)),
                                Span::styled(glyph, Style::default().fg(glyph_color)),
                                Span::styled(format!(" {} ", type_tag), Style::default().fg(glyph_color).add_modifier(ratatui::style::Modifier::BOLD)),
                                Span::styled(trunc_name, Style::default().fg(warm_white)),
                            ]));

                            // Location line: coord + district
                            let coord_str = format!("[{},{},{}]", room[0], room[1], room[2]);
                            let district_display = district.replace('_', " ");
                            lines.push(Line::from(vec![
                                Span::styled(format!("     {} ", coord_str), Style::default().fg(Color::Rgb(100, 95, 85))),
                                Span::styled(district_display, Style::default().fg(Color::Rgb(80, 75, 65))),
                            ]));
                        }
                    }

                    let para = Paragraph::new(lines).block(block);
                    para.render(right_area, buf);
                }
                PauseView::Controls => {
                    // ── Controls sub-screen ──
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Controls ", Style::default().fg(black).bg(accent))
                            ).alignment(Alignment::Left)
                        )
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Esc back ", Style::default().fg(dark))
                            ).alignment(Alignment::Right)
                        )
                        .border_style(Style::default().fg(accent));

                    let controls = vec![
                        ("Movement", vec![
                            ("W / X", "Move up / down"),
                            ("A / D", "Move left / right"),
                            ("Q / E", "Move NW / NE"),
                            ("Z / C", "Move SW / SE"),
                        ]),
                        ("Vertical", vec![
                            ("R / \u{2191}", "Ascend shaft"),
                            ("F / \u{2193}", "Descend shaft"),
                        ]),
                        ("Interaction", vec![
                            ("Space", "Interact / read / use"),
                            ("G", "Gather text to inventory"),
                        ]),
                        ("Interface", vec![
                            ("Tab", "Cycle HUD mode"),
                            ("\u{2190} / \u{2192}", "Flip texts on shelf"),
                            ("W / X", "Scroll in popups"),
                            ("Esc", "Pause menu / close popup"),
                        ]),
                    ];

                    let mut lines: Vec<Line> = Vec::new();
                    for (section, bindings) in &controls {
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            format!("  {}", section),
                            Style::default().fg(accent),
                        )));
                        for (key, desc) in bindings {
                            lines.push(Line::from(vec![
                                Span::styled(format!("    {:12}", key), Style::default().fg(warm_white)),
                                Span::styled(*desc, Style::default().fg(mid_gray)),
                            ]));
                        }
                    }

                    let para = Paragraph::new(lines).block(block);
                    para.render(right_area, buf);
                }
                PauseView::Sessions => {
                    // ── Session browser ──
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Sessions ", Style::default().fg(black).bg(accent))
                            ).alignment(Alignment::Left)
                        )
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Enter resume  Esc back ", Style::default().fg(dark))
                            ).alignment(Alignment::Right)
                        )
                        .border_style(Style::default().fg(accent));

                    let r_inner_h = menu_height.saturating_sub(2) as usize;
                    let r_inner_w = right_width.saturating_sub(4) as usize;
                    let mut lines: Vec<Line> = Vec::new();

                    if *sessions_loading {
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "  Loading sessions...",
                            Style::default().fg(mid_gray),
                        )));
                    } else if session_list.is_empty() {
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "  No prior sessions found.",
                            Style::default().fg(dark),
                        )));
                        lines.push(Line::from(""));
                        lines.push(Line::from(Span::styled(
                            "  Talk to the Librarian to",
                            Style::default().fg(Color::Rgb(60, 55, 50)),
                        )));
                        lines.push(Line::from(Span::styled(
                            "  start a session.",
                            Style::default().fg(Color::Rgb(60, 55, 50)),
                        )));
                    } else {
                        lines.push(Line::from("")); // top padding

                        // Scroll: ensure selected is visible
                        let visible_count = r_inner_h.saturating_sub(2); // top padding + bottom hint
                        let skip = if *session_selected >= visible_count {
                            session_selected + 1 - visible_count
                        } else {
                            0
                        };

                        for (i, sess) in session_list.iter().enumerate().skip(skip).take(visible_count) {
                            let is_sel = i == *session_selected;
                            let pointer = if is_sel { "\u{25b8} " } else { "  " };
                            let ptr_color = if is_sel { accent } else { dark };
                            let title_color = if is_sel { warm_white } else { mid_gray };
                            let meta_color = if is_sel { dim_gold } else { dark };

                            // Title line (truncated)
                            let display_title = if sess.title.len() > r_inner_w.saturating_sub(6) {
                                let end = sess.title.floor_char_boundary(r_inner_w.saturating_sub(9));
                                format!("{}...", &sess.title[..end])
                            } else {
                                sess.title.clone()
                            };

                            lines.push(Line::from(vec![
                                Span::styled(format!("  {}", pointer), Style::default().fg(ptr_color)),
                                Span::styled(display_title, Style::default().fg(title_color)),
                            ]));

                            // Metadata line: timestamp + message count
                            let ts_short = if sess.started_at.len() >= 16 {
                                &sess.started_at[..16]
                            } else {
                                &sess.started_at
                            };
                            lines.push(Line::from(Span::styled(
                                format!("      {} \u{00b7} {} msgs", ts_short, sess.message_count),
                                Style::default().fg(meta_color),
                            )));

                            if i + 1 < session_list.len() {
                                lines.push(Line::from("")); // spacing
                            }
                        }

                        // Scroll indicators
                        if skip > 0 {
                            lines.insert(1, Line::from(Span::styled(
                                "  \u{25b2} more above",
                                Style::default().fg(Color::Rgb(60, 55, 50)),
                            )));
                        }
                        if skip + visible_count < session_list.len() {
                            lines.push(Line::from(Span::styled(
                                "  \u{25bc} more below",
                                Style::default().fg(Color::Rgb(60, 55, 50)),
                            )));
                        }
                    }

                    let para = Paragraph::new(lines).block(block);
                    para.render(right_area, buf);
                }
                PauseView::CorpusSearch => {
                    // ── Corpus text search ──
                    let search_cyan = Color::Rgb(80, 200, 200);
                    let dim_search = Color::Rgb(50, 130, 130);

                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Corpus Search ", Style::default().fg(black).bg(search_cyan))
                            ).alignment(Alignment::Left)
                        )
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Space read  G gather  Esc back ", Style::default().fg(dark))
                            ).alignment(Alignment::Right)
                        )
                        .border_style(Style::default().fg(search_cyan));

                    let r_inner_h = menu_height.saturating_sub(2) as usize;
                    let r_inner_w = right_width.saturating_sub(4) as usize;
                    let mut lines: Vec<Line> = Vec::new();

                    // Search input
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("  \u{276f} ", Style::default().fg(search_cyan)),
                        Span::styled(search_input.as_str(), Style::default().fg(warm_white)),
                        Span::styled("\u{258c}", Style::default().fg(search_cyan)),
                    ]));
                    lines.push(Line::from(""));

                    if *search_loading {
                        lines.push(Line::from(Span::styled(
                            "  Searching...",
                            Style::default().fg(mid_gray),
                        )));
                    } else if search_results.is_empty() && !search_input.is_empty() {
                        lines.push(Line::from(Span::styled(
                            "  No results. Try different terms.",
                            Style::default().fg(dark),
                        )));
                    } else if search_results.is_empty() {
                        lines.push(Line::from(Span::styled(
                            "  Type a query and press Enter.",
                            Style::default().fg(Color::Rgb(60, 55, 50)),
                        )));
                    } else {
                        // Results list with selected item preview
                        let result_area = r_inner_h.saturating_sub(4); // top padding + input + spacing
                        // Split: left half = result list, but since we're in a single panel,
                        // show list with selected item's detail below

                        // Scroll: ensure selected is visible (show ~5 results at a time)
                        let list_height = result_area.min(search_results.len()).min(8);
                        let detail_height = result_area.saturating_sub(list_height + 1);

                        let skip = if *search_selected >= list_height {
                            search_selected + 1 - list_height
                        } else {
                            0
                        };

                        for (i, hit) in search_results.iter().enumerate().skip(skip).take(list_height) {
                            let is_sel = i == *search_selected;
                            let ptr = if is_sel { "\u{25b8} " } else { "  " };
                            let ptr_color = if is_sel { search_cyan } else { dark };
                            let name_color = if is_sel { warm_white } else { mid_gray };
                            let id_color = if is_sel { dim_search } else { Color::Rgb(60, 55, 50) };

                            // Truncate name to fit
                            let max_name = r_inner_w.saturating_sub(6);
                            let display_name = if hit.name.len() > max_name {
                                let end = hit.name.floor_char_boundary(max_name.saturating_sub(3));
                                format!("{}...", &hit.name[..end])
                            } else if hit.name.is_empty() {
                                hit.canonical_id.clone()
                            } else {
                                hit.name.clone()
                            };

                            lines.push(Line::from(vec![
                                Span::styled(format!("  {}", ptr), Style::default().fg(ptr_color)),
                                Span::styled(display_name, Style::default().fg(name_color)),
                            ]));
                            // Show ID on second line for selected
                            if is_sel && !hit.name.is_empty() {
                                lines.push(Line::from(Span::styled(
                                    format!("      {}", hit.canonical_id),
                                    Style::default().fg(id_color),
                                )));
                            }
                        }

                        // Scroll indicators
                        if skip > 0 {
                            lines.insert(3, Line::from(Span::styled(
                                "  \u{25b2} more above",
                                Style::default().fg(Color::Rgb(60, 55, 50)),
                            )));
                        }
                        if skip + list_height < search_results.len() {
                            lines.push(Line::from(Span::styled(
                                format!("  \u{25bc} {} more below", search_results.len() - skip - list_height),
                                Style::default().fg(Color::Rgb(60, 55, 50)),
                            )));
                        }

                        // Detail panel for selected result
                        if let Some(hit) = search_results.get(*search_selected) {
                            lines.push(Line::from(""));
                            // Separator
                            let sep: String = "\u{2500}".repeat(r_inner_w.saturating_sub(2));
                            lines.push(Line::from(Span::styled(
                                format!("  {}", sep),
                                Style::default().fg(Color::Rgb(50, 45, 40)),
                            )));

                            // Snippet
                            if !hit.snippet.is_empty() {
                                let snippet_w = r_inner_w.saturating_sub(4);
                                let snippet_lines = wrap_to_width(&hit.snippet, snippet_w);
                                for sl in snippet_lines.iter().take(3) {
                                    lines.push(Line::from(Span::styled(
                                        format!("    {}", sl),
                                        Style::default().fg(warm_white),
                                    )));
                                }
                                if snippet_lines.len() > 3 {
                                    lines.push(Line::from(Span::styled(
                                        "    ...",
                                        Style::default().fg(mid_gray),
                                    )));
                                }
                            }

                            // Metadata
                            if !hit.district.is_empty() {
                                lines.push(Line::from(vec![
                                    Span::styled("    District: ", Style::default().fg(mid_gray)),
                                    Span::styled(hit.district.clone(), Style::default().fg(warm_white)),
                                ]));
                            }
                            if let Some(z) = hit.z_band {
                                lines.push(Line::from(vec![
                                    Span::styled("    Depth:    ", Style::default().fg(mid_gray)),
                                    Span::styled(format!("z={}", z), Style::default().fg(warm_white)),
                                ]));
                            }
                            if let Some(coord) = hit.coord {
                                lines.push(Line::from(vec![
                                    Span::styled("    Coord:    ", Style::default().fg(mid_gray)),
                                    Span::styled(
                                        format!("{},{},{}", coord[0], coord[1], coord[2]),
                                        Style::default().fg(dim_search),
                                    ),
                                ]));
                            }
                        }
                    }

                    let para = Paragraph::new(lines).block(block);
                    para.render(right_area, buf);
                }
                PauseView::Articles => {
                    // ── Article reader ──
                    let article_gold = Color::Rgb(220, 180, 80);
                    let dim_article = Color::Rgb(140, 110, 50);

                    if !article_body.is_empty() {
                        // ── Reading view: full article ──
                        // Extract title from first line for the border
                        let display_title = article_body.lines().next()
                            .map(|l| l.trim_start_matches('#').trim().trim_matches('"'))
                            .unwrap_or("Article");
                        let title_short = if display_title.len() > 40 {
                            let end = display_title.floor_char_boundary(37);
                            format!(" {}... ", &display_title[..end])
                        } else {
                            format!(" {} ", display_title)
                        };

                        let block = Block::default()
                            .borders(Borders::ALL)
                            .title(
                                ratatui::widgets::block::Title::from(
                                    Span::styled(title_short, Style::default().fg(black).bg(article_gold))
                                ).alignment(Alignment::Left)
                            )
                            .title(
                                ratatui::widgets::block::Title::from(
                                    Span::styled(" W/X scroll  Esc back ", Style::default().fg(dark))
                                ).alignment(Alignment::Right)
                            )
                            .border_style(Style::default().fg(article_gold));

                        let r_inner_h = menu_height.saturating_sub(2) as usize;
                        let r_inner_w = right_width.saturating_sub(4) as usize;
                        let text_w = r_inner_w.saturating_sub(2);

                        // Render markdown-ish content with colors
                        let mut body_lines: Vec<Line> = Vec::new();
                        for para in article_body.lines() {
                            let trimmed = para.trim();
                            if trimmed.is_empty() {
                                body_lines.push(Line::from(""));
                                continue;
                            }
                            let (prefix, color) = if trimmed.starts_with("### ") {
                                ("  ", dim_article)
                            } else if trimmed.starts_with("## ") {
                                ("  ", article_gold)
                            } else if trimmed.starts_with("# ") {
                                ("  ", article_gold)
                            } else if trimmed.starts_with("> ") {
                                ("    ", Color::Rgb(200, 190, 160))
                            } else if trimmed.starts_with("* ") || trimmed.starts_with("- ") {
                                ("  ", warm_white)
                            } else if trimmed.starts_with("---") || trimmed.starts_with("***") {
                                ("  ", Color::Rgb(70, 65, 55))
                            } else if trimmed.starts_with('*') && trimmed.ends_with('*') {
                                ("  ", mid_gray)  // italic metadata
                            } else {
                                ("  ", warm_white)
                            };
                            let padded = format!("{}{}", prefix, trimmed);
                            let wrapped = wrap_text(&padded, text_w);
                            for wline in wrapped {
                                body_lines.push(Line::from(Span::styled(wline, Style::default().fg(color))));
                            }
                        }

                        // Scroll logic — article_scroll=0 means top of article
                        let skip = *article_scroll;
                        let visible: Vec<Line> = body_lines.into_iter().skip(skip).take(r_inner_h).collect();

                        let para = Paragraph::new(visible).block(block);
                        para.render(right_area, buf);
                    } else {
                        // ── List view: article titles ──
                        let block = Block::default()
                            .borders(Borders::ALL)
                            .title(
                                ratatui::widgets::block::Title::from(
                                    Span::styled(
                                        format!(" Articles \u{2014} {} ", article_list.len()),
                                        Style::default().fg(black).bg(article_gold),
                                    )
                                ).alignment(Alignment::Left)
                            )
                            .title(
                                ratatui::widgets::block::Title::from(
                                    Span::styled(" Enter read  Esc back ", Style::default().fg(dark))
                                ).alignment(Alignment::Right)
                            )
                            .border_style(Style::default().fg(article_gold));

                        let r_inner_h = menu_height.saturating_sub(2) as usize;
                        let r_inner_w = right_width.saturating_sub(4) as usize;
                        let mut lines: Vec<Line> = Vec::new();

                        if article_list.is_empty() {
                            lines.push(Line::from(""));
                            lines.push(Line::from(Span::styled(
                                "  No articles found.",
                                Style::default().fg(dark),
                            )));
                            lines.push(Line::from(""));
                            lines.push(Line::from(Span::styled(
                                "  Gather texts and invoke the",
                                Style::default().fg(Color::Rgb(60, 55, 50)),
                            )));
                            lines.push(Line::from(Span::styled(
                                "  Scribe to create articles.",
                                Style::default().fg(Color::Rgb(60, 55, 50)),
                            )));
                        } else {
                            lines.push(Line::from("")); // top padding

                            // Scroll: ensure selected is visible
                            let visible_count = r_inner_h.saturating_sub(2);
                            let skip = if *article_selected >= visible_count {
                                article_selected + 1 - visible_count
                            } else {
                                0
                            };

                            for (i, art) in article_list.iter().enumerate().skip(skip).take(visible_count) {
                                let is_sel = i == *article_selected;
                                let pointer = if is_sel { "\u{25b8} " } else { "  " };
                                let ptr_color = if is_sel { article_gold } else { dark };
                                let title_color = if is_sel { warm_white } else { mid_gray };
                                let meta_color = if is_sel { dim_article } else { dark };

                                // Truncate title
                                let max_title = r_inner_w.saturating_sub(6);
                                let display_title = if art.title.len() > max_title {
                                    let end = art.title.floor_char_boundary(max_title.saturating_sub(3));
                                    format!("{}...", &art.title[..end])
                                } else {
                                    art.title.clone()
                                };

                                lines.push(Line::from(vec![
                                    Span::styled(format!("  {}", pointer), Style::default().fg(ptr_color)),
                                    Span::styled(display_title, Style::default().fg(title_color)),
                                ]));

                                // Metadata: word count + date
                                lines.push(Line::from(Span::styled(
                                    format!("      {} words \u{00b7} {}", art.word_count, art.date),
                                    Style::default().fg(meta_color),
                                )));

                                if i + 1 < article_list.len() {
                                    lines.push(Line::from("")); // spacing
                                }
                            }

                            // Scroll indicators
                            if skip > 0 {
                                lines.insert(1, Line::from(Span::styled(
                                    "  \u{25b2} more above",
                                    Style::default().fg(Color::Rgb(60, 55, 50)),
                                )));
                            }
                            if skip + visible_count < article_list.len() {
                                lines.push(Line::from(Span::styled(
                                    "  \u{25bc} more below",
                                    Style::default().fg(Color::Rgb(60, 55, 50)),
                                )));
                            }
                        }

                        let para = Paragraph::new(lines).block(block);
                        para.render(right_area, buf);
                    }
                }
                PauseView::RoomLookup => {
                    // ── Room coordinate lookup ──
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Room Lookup ", Style::default().fg(black).bg(accent))
                            ).alignment(Alignment::Left)
                        )
                        .title(
                            ratatui::widgets::block::Title::from(
                                Span::styled(" Esc back ", Style::default().fg(dark))
                            ).alignment(Alignment::Right)
                        )
                        .border_style(Style::default().fg(accent));

                    let mut lines: Vec<Line> = Vec::new();
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "  Enter coordinates (q,r,z):",
                        Style::default().fg(mid_gray),
                    )));
                    lines.push(Line::from(""));
                    // Input line with cursor
                    lines.push(Line::from(vec![
                        Span::styled("  \u{276f} ", Style::default().fg(accent)),
                        Span::styled(coord_input.as_str(), Style::default().fg(warm_white)),
                        Span::styled("\u{258c}", Style::default().fg(accent)),
                    ]));
                    lines.push(Line::from(""));

                    // Result display
                    if let Some(ref result) = lookup_result {
                        if let Some(ref err) = result.error {
                            lines.push(Line::from(Span::styled(
                                format!("  \u{2717} {}", err),
                                Style::default().fg(Color::Rgb(200, 80, 60)),
                            )));
                        } else {
                            lines.push(Line::from(vec![
                                Span::styled("  \u{2713} ", Style::default().fg(Color::Rgb(80, 200, 80))),
                                Span::styled(result.name.clone(), Style::default().fg(warm_white)),
                            ]));
                            lines.push(Line::from(vec![
                                Span::styled("    ", Style::default()),
                                Span::styled(
                                    format!("[{},{},{}]", result.coord[0], result.coord[1], result.coord[2]),
                                    Style::default().fg(dark),
                                ),
                            ]));
                            lines.push(Line::from(""));
                            lines.push(Line::from(vec![
                                Span::styled("    District: ", Style::default().fg(mid_gray)),
                                Span::styled(result.district.clone(), Style::default().fg(warm_white)),
                            ]));
                            lines.push(Line::from(vec![
                                Span::styled("    Era:      ", Style::default().fg(mid_gray)),
                                Span::styled(result.era.clone(), Style::default().fg(warm_white)),
                            ]));
                            lines.push(Line::from(vec![
                                Span::styled("    Volumes:  ", Style::default().fg(mid_gray)),
                                Span::styled(
                                    format!("{}", result.volume_count),
                                    Style::default().fg(accent),
                                ),
                            ]));
                            if !result.composition.is_empty() {
                                lines.push(Line::from(vec![
                                    Span::styled("    Content:  ", Style::default().fg(mid_gray)),
                                    Span::styled(result.composition.clone(), Style::default().fg(warm_white)),
                                ]));
                            }
                            if !result.cluster_sig.is_empty() {
                                let sig = result.cluster_sig.join(" \u{00b7} ");
                                lines.push(Line::from(vec![
                                    Span::styled("    Lemmata:  ", Style::default().fg(mid_gray)),
                                    Span::styled(sig, Style::default().fg(accent)),
                                ]));
                            }
                            lines.push(Line::from(""));
                            lines.push(Line::from(Span::styled(
                                "  Enter to navigate here",
                                Style::default().fg(dim_gold),
                            )));
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            "  e.g. 0,0,3 or 4,-16,0",
                            Style::default().fg(Color::Rgb(60, 55, 50)),
                        )));
                    }

                    let para = Paragraph::new(lines).block(block);
                    para.render(right_area, buf);
                }
                PauseView::Map => {
                    // Handled by fullscreen intercept above; unreachable
                }
            }
        }
        PopupContent::Chat {
            messages,
            input_buffer,
            input_cursor,
            streaming_partial,
            streaming_is_thinking,
            status,
            scroll_offset,
            status_text,
        } => {
            let magenta = Color::Magenta;
            let dark_gray = Color::Rgb(60, 60, 60);
            let dim_magenta = Color::Rgb(140, 80, 140);
            let thinking_gray = Color::Rgb(120, 115, 110);
            let thinking_magenta = Color::Rgb(200, 100, 200);

            // Centered full-width popup (same size as lectern/shelf popups)
            Clear.render(popup_area, buf);

            // Status indicator for title bar
            let status_indicator = match status {
                ChatStatus::Idle => "",
                ChatStatus::WaitingForAgent => " thinking...",
                ChatStatus::Streaming => " thinking...",
                ChatStatus::Error(_) => " error",
            };

            // Title: "Hermes Agent" highlighted (black on magenta), metadata badge dimmer
            let title_spans = if let Some(meta) = chat_title {
                vec![
                    Span::styled(" Hermes Agent ", Style::default().fg(black).bg(magenta)),
                    Span::styled(
                        meta,
                        Style::default().fg(black).bg(dim_magenta),
                    ),
                    Span::styled(status_indicator, Style::default().fg(dim_magenta)),
                    Span::raw(" "),
                ]
            } else {
                vec![
                    Span::styled(" Hermes Agent ", Style::default().fg(black).bg(magenta)),
                    Span::styled(status_indicator, Style::default().fg(dim_magenta)),
                    Span::raw(" "),
                ]
            };

            let block = Block::default()
                .borders(Borders::ALL)
                .title(
                    ratatui::widgets::block::Title::from(title_spans)
                        .alignment(Alignment::Left)
                )
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" \u{2191}/\u{2193} scroll | Esc close ", Style::default().fg(mid_gray))
                    ).alignment(Alignment::Right)
                )
                .border_style(Style::default().fg(magenta));

            // Inner dimensions (minus borders)
            let inner_width = popup_width.saturating_sub(2) as usize;
            let pad_width = inner_width.saturating_sub(4); // 2 chars padding each side
            let inner_height = popup_height.saturating_sub(2) as usize;

            // Reserve bottom: separator(1) + input lines + optional status(1)
            let has_streaming = !streaming_partial.is_empty();
            let has_status = status_text.is_some() || has_streaming;
            // Calculate how many lines the input will take when wrapped
            let prompt_display_len = super::text_util::display_width(" \u{276f} ") + super::text_util::display_width(input_buffer); // display cols
            let wrap_width = inner_width.saturating_sub(1).max(1);
            let input_line_count = ((prompt_display_len + wrap_width - 1) / wrap_width).max(1);
            let mut reserved_bottom = 1 + input_line_count; // separator + input lines
            if has_status {
                reserved_bottom += 1;
            }
            let msg_area_height = inner_height.saturating_sub(reserved_bottom);

            // --- Build message lines with word wrapping ---
            let mut msg_lines: Vec<Line> = Vec::new();
            for (i, msg) in messages.iter().enumerate() {
                if i > 0 {
                    msg_lines.push(Line::from("")); // spacing between messages
                }
                match msg.role {
                    ChatRole::User => {
                        // User messages: gold prefix, then wrapped text
                        let first_line = format!("  You: {}", msg.text);
                        let wrapped = wrap_text(&first_line, pad_width);
                        for (j, wline) in wrapped.into_iter().enumerate() {
                            msg_lines.push(Line::from(Span::styled(wline, Style::default().fg(gold))));
                        }
                    }
                    ChatRole::Librarian => {
                        // Thinking messages: grey. Content messages: warm white.
                        let color = if msg.is_thinking { thinking_gray } else { white };
                        for para in msg.text.split('\n') {
                            if para.trim().is_empty() {
                                msg_lines.push(Line::from(""));
                                continue;
                            }
                            let padded = format!("  {}", para);
                            let wrapped = wrap_text(&padded, pad_width);
                            for wline in wrapped {
                                msg_lines.push(Line::from(Span::styled(wline, Style::default().fg(color))));
                            }
                        }
                    }
                    ChatRole::System => {
                        // System: dim, italic-style
                        let full = format!("  * {}", msg.text);
                        let wrapped = wrap_text(&full, pad_width);
                        for wline in wrapped {
                            msg_lines.push(Line::from(Span::styled(
                                wline,
                                Style::default().fg(mid_gray),
                            )));
                        }
                    }
                }
            }

            // Add streaming partial as an in-progress librarian message
            if has_streaming {
                if !messages.is_empty() {
                    msg_lines.push(Line::from(""));
                }
                // Thinking stream: magenta text. Content stream: white.
                let stream_color = if *streaming_is_thinking { thinking_magenta } else { white };
                let padded = format!("  {}", streaming_partial);
                let wrapped = wrap_text(&padded, pad_width.saturating_sub(1)); // room for cursor
                let last_idx = wrapped.len().saturating_sub(1);
                for (j, wline) in wrapped.into_iter().enumerate() {
                    if j == last_idx {
                        // Append blinking cursor to last line
                        msg_lines.push(Line::from(vec![
                            Span::styled(wline, Style::default().fg(stream_color)),
                            Span::styled("\u{258c}", Style::default().fg(magenta)),
                        ]));
                    } else {
                        msg_lines.push(Line::from(Span::styled(wline, Style::default().fg(stream_color))));
                    }
                }
            }

            // --- Scroll logic ---
            // scroll_offset=0 means "show latest" (auto-scroll to bottom)
            // scroll_offset>0 means "scrolled N lines up from bottom"
            let total_msg_lines = msg_lines.len();
            let auto_bottom = total_msg_lines.saturating_sub(msg_area_height);
            let skip = if *scroll_offset > 0 {
                auto_bottom.saturating_sub(*scroll_offset)
            } else {
                auto_bottom
            };

            let visible_msgs: Vec<Line> = msg_lines
                .into_iter()
                .skip(skip)
                .take(msg_area_height)
                .collect();

            // --- Assemble all lines ---
            let mut all_lines: Vec<Line> = Vec::new();

            // Scroll-up indicator
            if skip > 0 {
                let hint = format!("  \u{25b2} {} more lines above \u{25b2}", skip);
                all_lines.push(Line::from(Span::styled(hint, Style::default().fg(dark_gray))));
                // Fill remaining msg area minus this indicator line
                let remaining = msg_area_height.saturating_sub(1);
                for line in visible_msgs.iter().take(remaining) {
                    all_lines.push(line.clone());
                }
                while all_lines.len() < msg_area_height {
                    all_lines.push(Line::from(""));
                }
            } else {
                // No scroll indicator — fill message area
                for line in &visible_msgs {
                    all_lines.push(line.clone());
                }
                while all_lines.len() < msg_area_height {
                    all_lines.push(Line::from(""));
                }
            }

            // Status line (tool progress / thinking)
            if let Some(st) = status_text {
                all_lines.push(Line::from(Span::styled(
                    format!("  \u{2192} {}", st),
                    Style::default().fg(dim_magenta),
                )));
            } else if matches!(status, ChatStatus::WaitingForAgent) && !has_streaming {
                all_lines.push(Line::from(Span::styled(
                    "  \u{2192} thinking...",
                    Style::default().fg(dim_magenta),
                )));
            } else if has_status && status_text.is_none() {
                // Streaming but no explicit status — show generic
                all_lines.push(Line::from(""));
            }

            // Separator + wrapped input with positioned cursor
            let separator: String = "\u{2500}".repeat(inner_width);
            all_lines.push(Line::from(Span::styled(separator, Style::default().fg(dark_gray))));

            let input_locked = matches!(status, ChatStatus::WaitingForAgent | ChatStatus::Streaming);
            let prompt_str = " \u{276f} ";
            let prompt_len = prompt_str.len();
            let line_width = inner_width.saturating_sub(1); // leave room for cursor at line end

            if input_locked {
                all_lines.push(Line::from(vec![
                    Span::styled(prompt_str, Style::default().fg(dim_magenta)),
                    Span::styled("...", Style::default().fg(mid_gray)),
                ]));
            } else {
                // Build flat display string: " ❯ " + input_buffer
                let display = format!("{}{}", prompt_str, input_buffer);
                let cursor_flat = prompt_len + *input_cursor;

                // Wrap into fixed-width lines
                let mut lines_out: Vec<String> = Vec::new();
                let mut pos = 0;
                while pos < display.len() {
                    let end = display.floor_char_boundary((pos + line_width).min(display.len()));
                    lines_out.push(display[pos..end].to_string());
                    pos = end;
                }
                if lines_out.is_empty() {
                    lines_out.push(prompt_str.to_string());
                }

                // Render each wrapped line; insert cursor block on the correct line
                let mut flat_pos = 0usize;
                for (li, line_str) in lines_out.iter().enumerate() {
                    let line_end = flat_pos + line_str.len();
                    let cursor_here = cursor_flat >= flat_pos && cursor_flat <= line_end;

                    if cursor_here {
                        let c = line_str.floor_char_boundary(cursor_flat - flat_pos);
                        let before = line_str[..c].to_string();
                        let after = line_str[c..].to_string();
                        if li == 0 {
                            let prompt_end = before.floor_char_boundary(prompt_len.min(c));
                            let prompt_part = before[..prompt_end].to_string();
                            let text_part = before[prompt_end..].to_string();
                            all_lines.push(Line::from(vec![
                                Span::styled(prompt_part, Style::default().fg(magenta)),
                                Span::styled(text_part, Style::default().fg(white)),
                                Span::styled("\u{258c}", Style::default().fg(magenta)),
                                Span::styled(after, Style::default().fg(white)),
                            ]));
                        } else {
                            all_lines.push(Line::from(vec![
                                Span::styled(before, Style::default().fg(white)),
                                Span::styled("\u{258c}", Style::default().fg(magenta)),
                                Span::styled(after, Style::default().fg(white)),
                            ]));
                        }
                    } else {
                        if li == 0 {
                            let prompt_end = line_str.floor_char_boundary(prompt_len.min(line_str.len()));
                            all_lines.push(Line::from(vec![
                                Span::styled(line_str[..prompt_end].to_string(), Style::default().fg(magenta)),
                                Span::styled(line_str[prompt_end..].to_string(), Style::default().fg(white)),
                            ]));
                        } else {
                            all_lines.push(Line::from(Span::styled(
                                line_str.to_string(), Style::default().fg(white),
                            )));
                        }
                    }
                    flat_pos = line_end;
                }
            }

            let paragraph = Paragraph::new(all_lines)
                .block(block)
                .wrap(Wrap { trim: false });
            paragraph.render(popup_area, buf);
        }
        PopupContent::ArticleRead { title, body, scroll_offset } => {
            let article_gold = Color::Rgb(220, 180, 80);
            let warm_white = Color::Rgb(240, 235, 220);

            Clear.render(popup_area, buf);

            let title_short = if title.len() > 50 {
                let end = title.floor_char_boundary(47);
                format!(" {}... ", &title[..end])
            } else {
                format!(" {} ", title)
            };

            let block = Block::default()
                .borders(Borders::ALL)
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(title_short, Style::default().fg(black).bg(article_gold))
                    ).alignment(Alignment::Left)
                )
                .title(
                    ratatui::widgets::block::Title::from(
                        Span::styled(" W/X scroll  Esc close ", Style::default().fg(mid_gray))
                    ).alignment(Alignment::Right)
                )
                .border_style(Style::default().fg(article_gold));

            let inner_height = popup_height.saturating_sub(2) as usize;
            let inner_width = popup_width.saturating_sub(2) as usize;
            let text_w = inner_width.saturating_sub(2);

            // Render markdown-ish content
            let mut body_lines: Vec<Line> = Vec::new();
            for para in body.lines() {
                let trimmed = para.trim();
                if trimmed.is_empty() {
                    body_lines.push(Line::from(""));
                    continue;
                }
                let (prefix, color) = if trimmed.starts_with("### ") {
                    ("  ", Color::Rgb(140, 110, 50))
                } else if trimmed.starts_with("## ") {
                    ("  ", article_gold)
                } else if trimmed.starts_with("# ") {
                    ("  ", article_gold)
                } else if trimmed.starts_with("> ") {
                    ("    ", Color::Rgb(200, 190, 160))
                } else if trimmed.starts_with("* ") || trimmed.starts_with("- ") {
                    ("  ", warm_white)
                } else if trimmed.starts_with("---") || trimmed.starts_with("***") {
                    ("  ", Color::Rgb(70, 65, 55))
                } else if trimmed.starts_with('*') && trimmed.ends_with('*') {
                    ("  ", mid_gray)
                } else {
                    ("  ", warm_white)
                };
                let padded = format!("{}{}", prefix, trimmed);
                let wrapped = wrap_text(&padded, text_w);
                for wline in wrapped {
                    body_lines.push(Line::from(Span::styled(wline, Style::default().fg(color))));
                }
            }

            // Scroll: offset=0 means top of article
            let skip = *scroll_offset;
            let visible: Vec<Line> = body_lines.into_iter().skip(skip).take(inner_height).collect();

            let paragraph = Paragraph::new(visible).block(block);
            paragraph.render(popup_area, buf);
        }
    }
}

/// Render minimized chat sidebar on the left edge — read-only, no input.
/// Shows latest messages + streaming. Player can move while this is visible.
pub fn render_chat_sidebar(
    messages: &[ChatMessage],
    streaming_partial: &str,
    status: &ChatStatus,
    status_text: &Option<String>,
    area: Rect,
    buf: &mut Buffer,
) {
    let magenta = Color::Magenta;
    let gold = Color::Rgb(196, 146, 42);
    let white = Color::Rgb(240, 235, 220);
    let mid_gray = Color::Rgb(140, 140, 140);
    let dim_magenta = Color::Rgb(140, 80, 140);

    // Narrow panel: left-anchored, leave 6 rows at bottom for compass
    let panel_width = 36u16.min(area.width / 3);
    let panel_height = area.height.saturating_sub(6);
    let panel_area = Rect::new(area.x, area.y, panel_width, panel_height);

    Clear.render(panel_area, buf);

    let status_indicator = match status {
        ChatStatus::Idle => "",
        ChatStatus::WaitingForAgent | ChatStatus::Streaming => " thinking...",
        ChatStatus::Error(_) => " error",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(
            ratatui::widgets::block::Title::from(vec![
                Span::styled(" Hermes", Style::default().fg(magenta)),
                Span::styled(status_indicator, Style::default().fg(dim_magenta)),
                Span::styled(" ", Style::default()),
            ]).alignment(Alignment::Left)
        )
        .border_style(Style::default().fg(Color::Rgb(80, 50, 80)));

    let inner_width = panel_width.saturating_sub(2) as usize;
    let pad_width = inner_width.saturating_sub(2);
    let inner_height = panel_height.saturating_sub(2) as usize;

    // Build message lines
    let mut msg_lines: Vec<Line> = Vec::new();
    for (i, msg) in messages.iter().enumerate() {
        if i > 0 {
            msg_lines.push(Line::from(""));
        }
        match msg.role {
            ChatRole::User => {
                let wrapped = wrap_text(&format!("  You: {}", msg.text), pad_width);
                for wl in wrapped {
                    msg_lines.push(Line::from(Span::styled(wl, Style::default().fg(gold))));
                }
            }
            ChatRole::Librarian => {
                let color = if msg.is_thinking { Color::Rgb(120, 115, 110) } else { white };
                for para in msg.text.split('\n') {
                    if para.trim().is_empty() { msg_lines.push(Line::from("")); continue; }
                    let wrapped = wrap_text(&format!("  {}", para), pad_width);
                    for wl in wrapped {
                        msg_lines.push(Line::from(Span::styled(wl, Style::default().fg(color))));
                    }
                }
            }
            ChatRole::System => {
                let wrapped = wrap_text(&format!("  * {}", msg.text), pad_width);
                for wl in wrapped {
                    msg_lines.push(Line::from(Span::styled(wl, Style::default().fg(mid_gray))));
                }
            }
        }
    }

    // Streaming
    if !streaming_partial.is_empty() {
        if !messages.is_empty() { msg_lines.push(Line::from("")); }
        let wrapped = wrap_text(&format!("  {}", streaming_partial), pad_width.saturating_sub(1));
        let last = wrapped.len().saturating_sub(1);
        for (j, wl) in wrapped.into_iter().enumerate() {
            if j == last {
                msg_lines.push(Line::from(vec![
                    Span::styled(wl, Style::default().fg(white)),
                    Span::styled("\u{258c}", Style::default().fg(magenta)),
                ]));
            } else {
                msg_lines.push(Line::from(Span::styled(wl, Style::default().fg(white))));
            }
        }
    }

    // Status
    if let Some(st) = status_text {
        msg_lines.push(Line::from(Span::styled(
            format!("  \u{2192} {}", st), Style::default().fg(dim_magenta),
        )));
    } else if matches!(status, ChatStatus::WaitingForAgent) && streaming_partial.is_empty() {
        msg_lines.push(Line::from(Span::styled(
            "  \u{2192} thinking...", Style::default().fg(dim_magenta),
        )));
    }

    // Show last N lines that fit (auto-scroll to bottom)
    let total = msg_lines.len();
    let skip = total.saturating_sub(inner_height);
    let visible: Vec<Line> = msg_lines.into_iter().skip(skip).take(inner_height).collect();

    let mut all_lines: Vec<Line> = Vec::new();
    // Pad top if content is short
    let pad_top = inner_height.saturating_sub(visible.len());
    for _ in 0..pad_top {
        all_lines.push(Line::from(""));
    }
    for line in visible {
        all_lines.push(line);
    }

    let paragraph = Paragraph::new(all_lines)
        .block(block)
        .wrap(Wrap { trim: false });
    paragraph.render(panel_area, buf);
}

/// Word-wrap a string to `width` display columns. No leading indent.
fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    super::text_util::wrap_to_cols(text, width)
}

/// Word-wrap preserving newlines and leading whitespace, using display columns.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    super::text_util::wrap_text(text, width)
}
