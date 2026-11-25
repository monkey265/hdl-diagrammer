# hdl-diagrammer Rust Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Python/Tk `vhdl_diagramer` with a Rust egui/eframe app that draws a VHDL or SystemVerilog top level as a routed block diagram.

**Architecture:** One cargo crate at the repo root. Pure-logic modules (`model`, `parser/*`, `router`, `history`) have no egui dependency and are unit-tested. `canvas` and `ui/*` render and edit the model. The Python tree is the porting reference and is deleted only in the last task.

**Tech Stack:** Rust stable, `eframe`/`egui`, `serde` + `serde_json`, `rfd` (file dialogs), `regex`.

**Spec:** `docs/superpowers/specs/2026-11-25-rust-rewrite-design.md`

## Global Constraints

- Crate/binary name `hdl-diagrammer`; run as `cargo run -- <file.vhd|file.sv>` or File > Open.
- Hand-written scanners for VHDL and SystemVerilog; no tree-sitter, no `sv-parser`.
- Single-file scope; instances are opaque blocks; no multi-file resolution.
- JSON save format stays compatible with the Python app: `{"instances":[...],"top_level_pins":[...]}` using the field names of `models.py` (`name, direction, signal, font_*` for ports; `name, entity, ports, x, y, width, height, color_override, locked, custom_width, custom_height, original_ports, visible, font_*, is_group, collapsed, children` for instances).
- Grid options 20 (default) and 40; `MIN_BLOCK_WIDTH=150`, `MIN_BLOCK_HEIGHT=90`, `GRID_STEP=10` (from `config.py`).
- Unsupported constructs produce a warning in `Design.warnings`; never panic, never silently drop a block.
- Recent files (max 10) stored in `~/.hdl_diagrammer_config.json` as `{"recent_files":[...]}`.
- No Claude/Anthropic co-author or "Generated with" lines in commits (user CLAUDE.md).

## Review Focus

1. Empty file, binary garbage, or a file with no entity/module: warning, empty diagram, no panic (Task 3, 5).
2. Mixed-case keywords, comments containing keywords (`-- entity foo`, `// module bar`, `/* ... */`) must not create blocks (Task 3, 5).
3. Positional SV connections and unconnected ports (`.p()`, `open`) must not shift or drop other ports (Task 3, 5).
4. Loading an old Python-produced JSON with missing optional fields must succeed with defaults (Task 1).
5. Routing when start/goal sit inside a blocked margin, or no path exists: returns a straight fallback, never loops forever (Task 6).

---

### Task 1: Crate scaffold and model with JSON compatibility

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/model.rs`, `tests/fixtures/python_diagram.json` (a diagram saved by the Python app; generate by running the Python app on `radio_top.vhd` and saving, or hand-trim to 2 instances)
- Test: `src/model.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `enum Direction { In, Out, InOut }` (serde-renamed `"IN"|"OUT"|"INOUT"`); `struct Port { name: String, direction: Direction, signal: String, font_family: String, font_size: u32, font_bold: bool, font_italic: bool }`; `struct Instance { name, entity, ports: Vec<Port>, x: i32, y: i32, width: i32, height: i32, color_override: Option<String>, locked: bool, custom_width: i32, custom_height: i32, original_ports: Vec<Port>, visible: bool, font_*, is_group: bool, collapsed: bool, children: Vec<Instance> }`; `struct Diagram { instances: Vec<Instance>, top_level_pins: Vec<Port> }` with `Diagram::from_json(&str) -> Result<Diagram, serde_json::Error>` and `to_json(&self) -> String`. Python defaults (font Arial/8/10, `visible=true`, instance font bold) via `#[serde(default = ...)]`. `parent` is not serialized.

- [ ] **Step 1: Write failing tests** `loads_python_json` (loads the fixture, asserts instance count and one port's `Direction::Out`), `missing_optional_fields_default` (JSON with only `name/entity/ports` per instance loads; `visible == true`, `width == 0`), `roundtrip_is_stable` (`from_json(to_json(d)) == d`).
- [ ] **Step 2:** `cargo test model` — expect compile failure / FAIL.
- [ ] **Step 3:** Create the crate (`cargo init --name hdl-diagrammer`), add deps, implement `model.rs` with serde derives, `PartialEq`, `Clone`. `main.rs` is a stub `fn main() {}` for now.
- [ ] **Step 4:** `cargo test model` — expect PASS.
- [ ] **Step 5:** Commit `feat: crate scaffold and serde model`.

### Task 2: Shared lexer helpers

**Files:**
- Create: `src/parser/mod.rs`, `src/parser/lex.rs`
- Test: `src/parser/lex.rs`

**Interfaces:**
- Produces: `fn strip_comments_vhdl(src: &str) -> String` (removes `--` to end of line); `fn strip_comments_sv(src: &str) -> String` (removes `//…` and `/*…*/`, keeps newlines so line counts survive; ignores comment markers inside string literals); `fn split_top_level(s: &str, sep: char) -> Vec<&str>` (splits on `sep` ignoring separators inside `()`, `[]`, `{}`); `struct Design { top_name: String, top_ports: Vec<Port>, instances: Vec<Instance>, signals: HashMap<String,String>, warnings: Vec<String> }`; `fn parse_file(path_or_ext: &str, text: &str) -> Design` dispatching on `.vhd/.vhdl` vs `.sv/.v`.

- [ ] **Step 1: Tests:** `vhdl_comment_hides_keyword` (`"-- entity foo\nx"` → no `entity`), `sv_block_comment_multiline`, `sv_string_with_slashes_kept`, `split_ignores_nested_commas` (`"a(1,2),b"` → 2 parts).
- [ ] **Step 2:** `cargo test lex` — FAIL.
- [ ] **Step 3:** Implement with a small char-state machine (not regex) for comments; `Design` and `parse_file` stubs returning an empty `Design` with a warning for unknown extensions.
- [ ] **Step 4:** PASS. **Step 5:** Commit `feat: lexer helpers`.

### Task 3: VHDL parser

**Files:**
- Create: `src/parser/vhdl.rs`, `tests/fixtures/` (copy `RC4_sync_top.vhd`, `radio_top.vhd` from repo root)
- Test: `src/parser/vhdl.rs`

**Interfaces:**
- Consumes: `strip_comments_vhdl`, `split_top_level`, `Design`.
- Produces: `pub fn parse_vhdl(text: &str) -> Design`. Reads (case-insensitive): top `entity … port (…)` with `in|out|inout|buffer`, optional `std_logic_vector(…)` types (ignored for name); `signal` declarations into `signals`; instantiations `label : [entity work.]name [generic map(…)] port map (…)`; named associations only for ports, positional ones produce a warning and are skipped. Direction for instance ports: look up the instantiated entity in the same file when present, else the name heuristic from `parser.py::_guess_direction`. `open` associations kept as ports with signal `open`.

- [ ] **Step 1: Tests:** `parses_entity_ports` (inline sample, asserts names/directions), `instance_count_radio_top` and `instance_count_rc4` (assert counts equal what the Python parser returns; run `VHDLParser` once and hardcode the numbers), `keyword_in_comment_ignored`, `mixed_case_keywords`, `open_association_kept`, `empty_input_warns` (empty string → empty design, one warning, no panic), `no_entity_warns`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. Port the signal/instance regexes from `vhdl_diagramer/parser.py` (`regex` crate) but split port maps with `split_top_level`. **Step 4:** PASS. **Step 5:** Commit `feat: vhdl parser`.

### Task 4: Router (pure logic)

**Files:**
- Create: `src/router.rs`, `src/geometry.rs`
- Test: both files

**Interfaces:**
- Produces: `type Pt = (i32, i32)`; `fn compress_polyline(pts: &[Pt]) -> Vec<Pt>` (port of `utils.py`); `struct Router { grid_step: i32 }` with `build_occupancy(&self, blocks: &[(i32,i32,i32,i32)], bounds: (i32,i32,i32,i32), margin: i32) -> HashMap<Pt,bool>` and `find_path(&self, start: Pt, goal: Pt, occ: &HashMap<Pt,bool>, wires: &HashMap<Pt, HashSet<String>>, signal: &str, bounds: (i32,i32,i32,i32)) -> Option<Vec<Pt>>` (A*, costs as in `routing.py`: blocked 1_000_000, same-signal 1, else `1 + 10*other_signals`); `fn snap(v: i32, grid: i32) -> i32`; `fn junctions(wires: &HashMap<Pt, HashSet<String>>) -> Vec<Pt>` (cells where one signal has ≥3 neighbours; port of `_draw_all_junctions` logic).

- [ ] **Step 1: Tests:** `compress_removes_collinear`, `find_path_around_block` (block between start and goal, path avoids occupied cells), `find_path_reuses_same_signal` (cheaper through own wire), `no_path_returns_none` (goal enclosed), `start_inside_margin_still_routes` (Review Focus 5), `snap_rounds_to_grid`, `junction_at_t_split`. Port the intent of `tests/test_grid_snapping.py`, `test_collision.py`, `test_junction_rendering.py`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement with `BinaryHeap`; cap iterations at cells in bounds so it terminates. **Step 4:** PASS. **Step 5:** Commit `feat: router and geometry`.

### Task 5: SystemVerilog parser

**Files:**
- Create: `src/parser/sv.rs`, `tests/fixtures/sample_top.sv` (module with ANSI ports, one non-ANSI submodule, named instance, positional instance, `.p()` unconnected, a `generate for` containing an instance, a `// module fake` comment)
- Test: `src/parser/sv.rs`

**Interfaces:**
- Consumes: `strip_comments_sv`, `split_top_level`, `Design`.
- Produces: `pub fn parse_sv(text: &str) -> Design`. Reads `module name [#(…)] (ports);` with ANSI (`input|output|inout [wire|logic|reg] [range] name`) and non-ANSI (port list plus body direction declarations); `wire|logic|reg` declarations into `signals`; instances `mod_name [#(…)] inst_name (conns);` with named `.p(expr)`, `.p()` (kept, signal empty), `.*` (warning), and positional (mapped by order against the instantiated module's ports if it is in the file, else warning). Instances inside `generate` get a warning "generate blocks not expanded" and are still listed once. Skips SV keywords so `if (…)`/`assign` are not read as instances. Top module = the one never instantiated in the file (first if ambiguous, with warning).

- [ ] **Step 1: Tests:** `ansi_ports_with_ranges`, `non_ansi_ports`, `named_instance_connections`, `positional_maps_by_order`, `unconnected_port_keeps_neighbours` (Review Focus 3), `comment_module_ignored`, `generate_instance_warns_but_listed`, `wildcard_warns`, `empty_input_warns`, `top_is_uninstantiated_module`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement as a token scanner over the comment-stripped text (identifiers, punctuation), not nested regex. **Step 4:** PASS. **Step 5:** Commit `feat: systemverilog parser`.

### Task 6: Layout and block sizing

**Files:**
- Create: `src/layout.rs`
- Test: `src/layout.rs`

**Interfaces:**
- Consumes: `Instance`, `snap`, config constants (`src/config.rs`, create with the constants from Global Constraints and the colour table from `config.py`).
- Produces: `fn block_size(inst: &Instance) -> (i32,i32)` (port of `calculate_block_size`, honours `custom_width/height`, min sizes); `fn arrange_grid(instances: &mut [Instance], grid: i32)` (port of `arrange_grid`); `fn port_pos(inst: &Instance, port_idx: usize) -> Pt` (IN on left edge, OUT on right, snapped to grid; port of `get_port_abs_pos` for non-gate blocks; ANSI gate variant deferred to Task 9).

- [ ] **Step 1: Tests:** `block_size_respects_minimum`, `custom_size_wins`, `arrange_grid_no_overlap`, `out_port_on_right_edge` (port of `test_port_position.py`), `ports_snap_to_grid`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS. **Step 5:** Commit `feat: layout`.

### Task 7: Read-only canvas and app shell

**Files:**
- Create: `src/app.rs`, `src/canvas.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `parse_file`, `Design`, `Diagram`, `arrange_grid`, `Router`, `port_pos`, `junctions`.
- Produces: `struct App` implementing `eframe::App`; `fn design_to_diagram(d: &Design) -> Diagram`; `Canvas { zoom: f32, pan: Vec2, grid: i32, show_grid: bool, show_signal_names: bool }` with `fn show(&mut self, ui: &mut egui::Ui, diagram: &Diagram)`; `fn route_all(diagram: &Diagram, grid: i32) -> Vec<Wire>` where `Wire { signal: String, segments: Vec<(Pt,Pt)> }` (same signal on ≥2 ports → one routed net; pure, unit-tested). CLI arg opens a file at start; File > Open uses `rfd`; warnings show in a bottom panel.

- [ ] **Step 1: Tests:** `route_all_connects_shared_signal` (two instances sharing a signal → one wire with ≥1 segment), `route_all_skips_single_port_signal`, `design_to_diagram_copies_original_ports`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement; render blocks, port pins (blue IN / red OUT from `COLORS`), wires, junction dots, grid background, scroll-wheel zoom, middle-drag pan, zoom-to-fit. **Step 4:** `cargo test` PASS, then `cargo run -- tests/fixtures/radio_top.vhd` and confirm blocks and wires appear (manual). **Step 5:** Commit `feat: read-only canvas`.

### Task 8: Interaction, undo/redo, save/load, inspector

**Files:**
- Create: `src/history.rs`, `src/ui/mod.rs`, `src/ui/inspector.rs`, `src/ui/menus.rs`
- Modify: `src/app.rs`, `src/canvas.rs`

**Interfaces:**
- Produces: `struct History<T: Clone> { fn push(&mut self, s: &T); fn undo(&mut self, cur: &T) -> Option<T>; fn redo(&mut self, cur: &T) -> Option<T> }`; canvas actions: select (click, shift multi-select), drag block with grid snap and collision push-out (skip when `locked`), right-click menu on block (colour, rename, size, lock, delete, restore, reset ports), on pin (colour, rename, delete), on wire (reset route, delete, bus style toggle); File > Save/Load JSON via `Diagram::to_json/from_json`; Open Recent (10 max, `~/.hdl_diagrammer_config.json`); Ctrl+Z / Ctrl+Y; inspector panel listing instances, ports, nets (port of `inspector_panel.py`).

- [ ] **Step 1: Tests:** `history_undo_redo_roundtrip`, `history_push_clears_redo`, `recent_files_dedup_and_cap_10`, `drag_snaps_to_grid`, `locked_block_does_not_move`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement; snapshot history on every completed edit. **Step 4:** `cargo test` PASS; manual: drag, undo, save, reload. **Step 5:** Commit `feat: interaction, history, save/load, inspector`.

### Task 9: Groups, collapse, ANSI gates

**Files:**
- Create: `src/group.rs`, `src/gates.rs`
- Modify: `src/layout.rs`, `src/canvas.rs`, `src/ui/menus.rs`

**Interfaces:**
- Produces: `fn create_group(d: &mut Diagram, name: &str, members: &[usize], ports: Vec<Port>)`; `fn ungroup(d: &mut Diagram, idx: usize)`; `fn toggle_collapse(d: &mut Diagram, idx: usize)`; `fn gate_kind(entity: &str) -> Option<GateKind>` (AND/OR/NOT/NAND/NOR/XOR/XNOR/BUF); gate-aware branch in `port_pos`; `fn draw_gate(painter, kind, rect)`. Port the logic of `create_group_from_selection`, `ungroup_selection`, `toggle_collapse`, `_draw_ansi_gate` and the group dialog (`GroupCreationDialog`).

- [ ] **Step 1: Tests:** `group_then_ungroup_restores`, `collapse_hides_children` (children `visible=false`), `group_ports_follow_dialog_list`, `gate_kind_recognises_names`, `gate_port_snaps_to_grid` (port of `test_grid_snapping::test_ansi_gate_port_snapping`).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS. **Step 5:** Commit `feat: groups, collapse, ansi gates`.

### Task 10: SV end to end, parity check, cleanup

**Files:**
- Modify: `README.md`, `.gitignore`, `.github/workflows/` (replace `pylint.yml` with a `cargo test` + `cargo clippy` workflow), `run_checks.sh`
- Delete (after parity check): `vhdl_diagramer/`, `tests/*.py`, `setup.py`, `verify_grid.py`, `build/`, `__pycache__/`, `vhdl_diagramer.egg-info/`, `vhdl_diagrammer_debug.log`

- [ ] **Step 1:** `cargo run -- tests/fixtures/sample_top.sv`; confirm the SV diagram renders and warnings show.
- [ ] **Step 2: Parity walk-through:** for each Python TODO/checked feature in the old `README.md` (move blocks, colour, drag lock, save/load, recent, undo/redo, groups, gates) confirm it works in the Rust app; list any gap in `README.md` under "Known gaps".
- [ ] **Step 3:** Delete the Python tree (`git rm -r`), rewrite `README.md` for cargo, update CI and `run_checks.sh` to `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.
- [ ] **Step 4:** Run `./run_checks.sh` — expect all pass.
- [ ] **Step 5:** Commit `chore: remove python implementation, rust is the app`.
