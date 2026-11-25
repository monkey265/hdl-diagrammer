# hdl-diagrammer

Draws a VHDL or SystemVerilog top level as a block diagram: instances, ports and routed nets.
Rust + egui/eframe; a rewrite of the earlier Python/Tk `vhdl-diagrammer`.

## Run

```
cargo run --release -- path/to/top.vhd     # or .sv / .v / .vhdl / a saved .json diagram
```

or start without an argument and use File > Open. Checks: `./run_checks.sh`.

## What it reads

Hand-written scanners, one file at a time. Instances of modules defined in other files are shown as
opaque blocks (pin directions guessed from port names). Anything it cannot handle is listed as a
warning at the bottom of the window instead of being dropped silently:
generate blocks (listed once, not expanded), `.*` connections, interface ports, macros and
`` `include ``, gate primitives.

## Using it

- Click / shift-click to select, drag blocks (grid snap, pushed off overlaps), middle-drag or drag the background to pan, scroll to zoom.
- Right-click a block, pin or the background: rename, colour, size, lock, delete / restore, reset ports, group, collapse.
- Click a wire (or a net in the inspector) to highlight it.
- Ctrl+Z / Ctrl+Y undo and redo. File > Save As writes JSON compatible with the old Python app.

## Known gaps vs the Python app

No per-pin colours, wire context actions (reset route, delete connection, bus style, manual route
dragging), resize handles, pin-to-pin wiring, dragging children inside an expanded group, font
bold/italic, the paste-VHDL dialog and `--debug`. VHDL `open` keyword is kept as a net named `open`
and not routed. Layouts for very large designs are not optimised.
