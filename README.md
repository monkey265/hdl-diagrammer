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

## Tcl scripting

Scripts run in an embedded [molt](https://crates.io/crates/molt) interpreter (a Tcl subset: `proc`, `foreach`,
`if`, `expr`, lists, `catch`, `puts`; no `namespace`/`package`). Blocks are addressed by instance name.

```
hdl-diagrammer --script run.tcl [file.vhd]        # headless, exit code 1 on a Tcl error
hdl-diagrammer file.vhd --script run.tcl --gui    # run, then open the window
```
The same commands run in the Transcript pane at the bottom of the window (`hdl>` prompt; Up/Down recall history, Tab completes command names), sharing the GUI's undo history. `puts` output appears there too. The window is laid out like Questa Sim: a dark theme, a Structure pane on the left (design hierarchy, search, click to go to a block), an Objects pane on the right (ports of the selected block, net list) and the Transcript below. Each pane can be hidden from the View menu; View > Dark theme switches to a light canvas.

| command | effect |
|---|---|
| `open f` / `save f.json` / `export f.svg` | load HDL or JSON / write JSON / write a picture (gates are plain boxes) |
| `instances`, `ports i`, `info i`, `nets`, `warnings` | queries, returned as Tcl lists |
| `rename i new`, `move i x y`, `color i #hex\|""`, `size i w h\|auto` | edit a block |
| `lock i`, `unlock i`, `hide i`, `show i` | block state |
| `group g i…`, `ungroup g`, `collapse g`, `expand g` | groups |
| `autolayout` | layered left-to-right layout (locked blocks stay) |
| `undo`, `redo` | one step per command |

Errors are ordinary Tcl errors (catchable with `catch`) and leave the diagram unchanged.
Children of an expanded group cannot be addressed by name yet.

## Reading big diagrams

- Files open in a layered layout (drivers left, loads right); Edit > Auto layout redoes it.
- Wires are coloured by class: clock purple, reset brown, bus blue (thicker), plain green, undeclared grey.
- View > "Clock/reset as stubs" (on by default) draws clock/reset nets and any net with 8+ pins as a short labelled stub at each pin instead of a routed wire.
- Click a wire to fade every other net. Each net is labelled once.
- Top-level pins with no connection are hollow; hide them with View > Unconnected pins.

## Projects, hierarchy and editing

- **Project files:** `open list.f` reads a file list (one path per line, `#` and `//` comments, `-top NAME` or `+top+NAME`). Without a list, the other HDL files next to the opened one are used. `open file ?top?` (Tcl) draws a chosen entity or module.
- **Hierarchy:** right-click a block > "Descend into" (Tcl: `descend inst`) opens the diagram of that block's entity from the project files; Tools > "Go up one level" (`ascend`) comes back. The Structure pane title shows the path.
- **Generate loops:** `for … generate` with literal bounds (VHDL and SystemVerilog) is expanded, each copy its own block (`u(0)`, `u[1]`). `if`/`case` generates and symbolic bounds are listed once with a warning.
- **Editing wires:** drag from one pin to another to connect them (they share a net; two existing nets merge). Right-click a port > Disconnect. Tcl: `connect a.port b.port`, `connect pin:name a.port`, `disconnect a.port`.
- **Export HDL:** File > Export HDL… (Tcl `exporthdl file.vhd|file.sv`) writes the diagram back as a top-level entity/module. Generics and sub-block port types are not reproduced; SystemVerilog ports are written as `logic`.
- **Saved views:** View > Bookmarks, or Tcl `view save|goto|delete|list name`. Stored next to the source as `<file>.views.json`.
- **Panes:** every pane's title-bar button (⧉) moves it left, right, bottom or into a floating window.
- Routing runs on a worker thread in the GUI, so large designs stay responsive; wires appear when ready.
