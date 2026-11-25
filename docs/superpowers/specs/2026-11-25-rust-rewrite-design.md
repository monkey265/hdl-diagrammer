# hdl-diagrammer: Rust rewrite (design)

## Intent
Rewrite the Python/Tk `vhdl-diagrammer` as **hdl-diagrammer**, a Rust egui/eframe desktop tool that visualises a VHDL or SystemVerilog top level as a block diagram (instances, ports, routed nets). The user asked for a full rewrite, egui/eframe, and VHDL + SystemVerilog support. The reason for the rewrite was not stated (my guess: single binary, speed); treat it as unconfirmed.

## Goals
- Feature parity with the Python app (see list), plus SystemVerilog input.
- Single static-ish binary: `cargo run -- <file.vhd|file.sv>`, also File > Open.
- Keep the existing JSON save format readable so old diagrams load (my call).
- Model, parser, router and rendering are separate modules; the router and parser are unit-tested without a GUI.

## Non-goals (first version)
- Multi-file project resolution (finding sub-modules across files). One file in, instances shown as opaque blocks. Revisit later.
- Generate-loop elaboration, interface ports, macro-built ports, `include/preprocessing (SV); `open` keyword and components (VHDL, already TODO in Python).
- Verilog-2001-only extras, export formats, tcl interface, signal rendering (Python TODO items).

## Anti-goals
- A full HDL parser. Unsupported constructs must be skipped with a visible warning, never crash or silently drop a block.

## Approaches considered
1. **Hand-written scanner for both languages — chosen.** Small, no heavy deps, keeps the JSON model. Misses exotic SV constructs.
2. `sv-parser` crate for SV: full IEEE 1800-2017 but slow to compile and a huge tree to mine for instances.
3. tree-sitter for both: robust on broken files but C build deps and uneven grammar quality.

## Design
Crate layout at repo root (Python tree removed once parity is reached):
- `model.rs`: `Port{name, dir, signal, font}`, `Instance{name, entity, ports, pos, size, color, locked, group/children, collapsed, visible, font}` mirroring `models.py`; serde for JSON.
- `parser/vhdl.rs`, `parser/sv.rs`: lexer (strip comments, strings, case-insensitive for VHDL) + scanners for entity/module ports and instantiations (port map / named and positional connections). Return `Design { top_ports, instances, signals, warnings }`.
- `router.rs`: grid occupancy + A* wire routing, polyline compression, junction detection (port of `routing.py` and the router in `diagram_canvas.py`).
- `app.rs` / `canvas.rs`: eframe app; custom-painted canvas with pan/zoom, grid snap (20/40), drag blocks, selection, undo/redo (snapshot stack), ANSI gate symbols, bus style, wire highlight.
- `ui/`: menu (open, save, load, recent), inspector panel, right-click menus (colour, rename, size, lock, delete port/instance, reset route), group create/ungroup/collapse dialog.
- Tests: port the Python tests that cover logic (grid snapping, collision, port position, models, junctions) as Rust unit tests; add parser fixtures from `radio_top.vhd` and `RC4_sync_top.vhd`, plus a small SV sample.

## Left to the builder
Exact egui crates for file dialogs (`rfd`), colour picker, recent-files storage location, and the order of porting (suggested: model, VHDL parser, router, read-only canvas, interaction, groups, SV parser).

## Calls I made (please check)
- Single-file scope; multi-file deferred.
- JSON compatibility kept.
- Rust crate goes at repo root; Python code deleted at the end, not before.
- GitHub repo/remote rename left to you.
