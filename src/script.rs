//! Tcl scripting (embedded `molt`): commands that read and edit the diagram by instance name.
use crate::edit;
use crate::group;
use crate::history::History;
use crate::model::{Diagram, Direction};
use crate::net::route_all;
use molt::Interp;
use molt::types::{ContextID, Exception, MoltResult, Value};
use std::collections::HashMap;
use std::path::Path;

/// Everything the commands act on. The GUI lends its state to a `Session` for each console line.
#[derive(Default)]
pub struct Session {
    pub diagram: Diagram,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
    pub grid: i32,
    pub history: History<Diagram>,
    /// Set by `open`; the GUI reacts by refitting the view and dropping its selection.
    pub opened: bool,
}

type Handler = fn(&mut Session, &[String]) -> Result<Value, String>;

fn list(items: impl IntoIterator<Item = String>) -> Value {
    Value::from(items.into_iter().map(Value::from).collect::<Vec<_>>())
}

fn find(d: &Diagram, name: &str) -> Result<usize, String> {
    d.instances
        .iter()
        .position(|i| i.name == name)
        .ok_or_else(|| format!("no such instance \"{name}\""))
}

fn int(s: &str) -> Result<i32, String> {
    s.parse()
        .map_err(|_| format!("expected an integer but got \"{s}\""))
}

impl Session {
    /// Run `f` as one undoable edit; a failure leaves the diagram untouched.
    fn edit(
        &mut self,
        f: impl FnOnce(&mut Diagram, i32) -> Result<(), String>,
    ) -> Result<Value, String> {
        let before = self.diagram.clone();
        match f(&mut self.diagram, self.grid) {
            Ok(()) => {
                self.history.push(&before);
                Ok(Value::empty())
            }
            Err(e) => {
                self.diagram = before;
                Err(e)
            }
        }
    }

    fn on(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut Diagram, usize, i32),
    ) -> Result<Value, String> {
        let i = find(&self.diagram, name)?;
        self.edit(|d, g| {
            f(d, i, g);
            Ok(())
        })
    }
}

fn open(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let l = crate::load::load(Path::new(&a[0]), s.grid)?;
    (s.diagram, s.signals, s.warnings) = (l.diagram, l.signals, l.warnings);
    s.history.clear();
    s.opened = true;
    Ok(Value::from(s.diagram.instances.len() as i64))
}

fn save(s: &mut Session, a: &[String]) -> Result<Value, String> {
    std::fs::write(&a[0], s.diagram.to_json()).map_err(|e| format!("cannot save {}: {e}", a[0]))?;
    Ok(Value::empty())
}

fn export(s: &mut Session, a: &[String]) -> Result<Value, String> {
    crate::svg::write(std::path::Path::new(&a[0]), &s.diagram, &s.signals)?;
    Ok(Value::empty())
}

fn instances(s: &mut Session, _: &[String]) -> Result<Value, String> {
    Ok(list(s.diagram.instances.iter().map(|i| i.name.clone())))
}

fn ports(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let i = &s.diagram.instances[find(&s.diagram, &a[0])?];
    Ok(Value::from(
        i.ports
            .iter()
            .map(|p| {
                let dir = match p.direction {
                    Direction::In => "in",
                    Direction::Out => "out",
                    Direction::InOut => "inout",
                };
                list([p.name.clone(), dir.into(), p.signal.clone()])
            })
            .collect::<Vec<_>>(),
    ))
}

fn info(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let i = &s.diagram.instances[find(&s.diagram, &a[0])?];
    let f = |b: bool| if b { "1" } else { "0" }.to_string();
    Ok(list([
        "entity".into(),
        i.entity.clone(),
        "x".into(),
        i.x.to_string(),
        "y".into(),
        i.y.to_string(),
        "width".into(),
        i.width.to_string(),
        "height".into(),
        i.height.to_string(),
        "locked".into(),
        f(i.locked),
        "visible".into(),
        f(i.visible),
        "group".into(),
        f(i.is_group),
        "collapsed".into(),
        f(i.collapsed),
    ]))
}

fn nets(s: &mut Session, _: &[String]) -> Result<Value, String> {
    let mut v: Vec<String> = route_all(&s.diagram)
        .into_iter()
        .map(|w| w.signal)
        .collect();
    v.sort();
    v.dedup();
    Ok(list(v))
}

fn warnings(s: &mut Session, _: &[String]) -> Result<Value, String> {
    Ok(list(s.warnings.clone()))
}

fn rename(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let i = find(&s.diagram, &a[0])?;
    if a[1].is_empty() || s.diagram.instances.iter().any(|x| x.name == a[1]) {
        return Err(format!("name \"{}\" is empty or already used", a[1]));
    }
    s.edit(|d, g| {
        d.instances[i].name = a[1].clone();
        edit::refit(d, i, g);
        Ok(())
    })
}

fn mv(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let (i, x, y) = (find(&s.diagram, &a[0])?, int(&a[1])?, int(&a[2])?);
    if s.diagram.instances[i].locked {
        return Err(format!("instance \"{}\" is locked", a[0]));
    }
    s.edit(|d, g| {
        edit::move_block(d, i, x, y, g);
        Ok(())
    })
}

fn color(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let c = a[1].clone();
    if !(c.is_empty() || c.starts_with('#') && matches!(c.len(), 4 | 7)) {
        return Err(format!(
            "colour \"{c}\" is not #rgb or #rrggbb (or \"\" to reset)"
        ));
    }
    s.on(&a[0], |d, i, _| {
        d.instances[i].color_override = Some(c).filter(|c| !c.is_empty());
    })
}

fn set_lock(s: &mut Session, name: &str, want: bool) -> Result<Value, String> {
    s.on(name, |d, i, _| {
        if d.instances[i].locked != want {
            edit::toggle_lock(d, i);
        }
    })
}

fn size(s: &mut Session, a: &[String]) -> Result<Value, String> {
    let sz = match a {
        [_, w] if w == "auto" => None,
        [_, w, h] => Some((int(w)?, int(h)?)),
        _ => return Err("usage: size inst w h | size inst auto".into()),
    };
    s.on(&a[0], |d, i, g| edit::set_size(d, i, sz, g))
}

fn make_group(s: &mut Session, a: &[String]) -> Result<Value, String> {
    if s.diagram.instances.iter().any(|x| x.name == a[0]) {
        return Err(format!("name \"{}\" is already used", a[0]));
    }
    let members = a[1..]
        .iter()
        .map(|n| find(&s.diagram, n))
        .collect::<Result<Vec<_>, _>>()?;
    s.edit(|d, _| {
        let ports = group::suggest_ports(d, &members);
        group::create_group(d, &a[0], &members, ports);
        Ok(())
    })
}

fn group_idx(s: &Session, name: &str) -> Result<usize, String> {
    let i = find(&s.diagram, name)?;
    if s.diagram.instances[i].is_group {
        Ok(i)
    } else {
        Err(format!("\"{name}\" is not a group"))
    }
}

fn set_collapsed(s: &mut Session, name: &str, want: bool) -> Result<Value, String> {
    let i = group_idx(s, name)?;
    s.edit(|d, g| {
        if d.instances[i].collapsed != want {
            group::toggle_collapse(d, i, g);
        }
        Ok(())
    })
}

fn undo(s: &mut Session, _: &[String]) -> Result<Value, String> {
    let prev = s.history.undo(&s.diagram).ok_or("nothing to undo")?;
    s.diagram = prev;
    Ok(Value::empty())
}

fn redo(s: &mut Session, _: &[String]) -> Result<Value, String> {
    let next = s.history.redo(&s.diagram).ok_or("nothing to redo")?;
    s.diagram = next;
    Ok(Value::empty())
}

fn validate(s: &mut Session, _: &[String]) -> Result<Value, String> {
    Ok(list(crate::validate::validate(&s.diagram)))
}

fn interfaces(s: &mut Session, a: &[String]) -> Result<Value, String> {
    match a[0].as_str() {
        "collapse" => s.edit(|d, g| {
            crate::interface::collapse_all(d, g);
            Ok(())
        }),
        "expand" => s.edit(|d, g| {
            crate::interface::expand_all(d, g);
            Ok(())
        }),
        _ => Err("usage: interfaces collapse|expand".into()),
    }
}

/// (name, usage after the name, min args, max args, handler)
const COMMANDS: &[(&str, &str, usize, usize, Handler)] = &[
    ("open", "file", 1, 1, open),
    ("save", "file.json", 1, 1, save),
    ("export", "file.svg|file.png", 1, 1, export),
    ("validate", "", 0, 0, validate),
    ("interfaces", "collapse|expand", 1, 1, interfaces),
    ("instances", "", 0, 0, instances),
    ("ports", "inst", 1, 1, ports),
    ("info", "inst", 1, 1, info),
    ("nets", "", 0, 0, nets),
    ("warnings", "", 0, 0, warnings),
    ("rename", "inst new", 2, 2, rename),
    ("move", "inst x y", 3, 3, mv),
    ("color", "inst #hex|\"\"", 2, 2, color),
    ("lock", "inst", 1, 1, |s, a| set_lock(s, &a[0], true)),
    ("unlock", "inst", 1, 1, |s, a| set_lock(s, &a[0], false)),
    ("hide", "inst", 1, 1, |s, a| {
        s.on(&a[0], |d, i, _| edit::set_visible(d, i, false))
    }),
    ("show", "inst", 1, 1, |s, a| {
        s.on(&a[0], |d, i, _| edit::set_visible(d, i, true))
    }),
    ("size", "inst w h | inst auto", 2, 3, size),
    ("group", "name inst ?inst ...?", 2, usize::MAX, make_group),
    (
        "addtogroup",
        "group inst ?inst ...?",
        2,
        usize::MAX,
        |s, a| {
            let g = group_idx(s, &a[0])?;
            let members = a[1..]
                .iter()
                .map(|n| find(&s.diagram, n))
                .collect::<Result<Vec<_>, _>>()?;
            if members.contains(&g) {
                return Err("a group cannot contain itself".into());
            }
            s.edit(|d, _| {
                group::add_to_group(d, g, &members);
                Ok(())
            })
        },
    ),
    ("release", "group inst", 2, 2, |s, a| {
        let g = group_idx(s, &a[0])?;
        let c = s.diagram.instances[g]
            .children
            .iter()
            .position(|c| c.name == a[1])
            .ok_or_else(|| format!("\"{}\" is not in group \"{}\"", a[1], a[0]))?;
        s.edit(|d, _| {
            group::release_child(d, g, c);
            Ok(())
        })
    }),
    ("deletegroup", "name", 1, 1, |s, a| {
        let i = group_idx(s, &a[0])?;
        s.edit(|d, _| {
            group::delete_group(d, i);
            Ok(())
        })
    }),
    ("ungroup", "name", 1, 1, |s, a| {
        let i = group_idx(s, &a[0])?;
        s.edit(|d, _| {
            group::ungroup(d, i);
            Ok(())
        })
    }),
    ("collapse", "group", 1, 1, |s, a| {
        set_collapsed(s, &a[0], true)
    }),
    ("expand", "group", 1, 1, |s, a| {
        set_collapsed(s, &a[0], false)
    }),
    ("autolayout", "", 0, 0, |s, _| {
        s.edit(|d, g| {
            crate::layout::auto_layout(d, g);
            Ok(())
        })
    }),
    ("undo", "", 0, 0, undo),
    ("redo", "", 0, 0, redo),
];

/// One entry point for every command: `argv[0]` picks the row of `COMMANDS`.
fn dispatch(interp: &mut Interp, ctx: ContextID, argv: &[Value]) -> MoltResult {
    let name = argv[0].as_str();
    let &(_, usage, min, max, handler) = COMMANDS
        .iter()
        .find(|c| c.0 == name)
        .expect("only registered names reach dispatch");
    let args: Vec<String> = argv[1..].iter().map(|v| v.as_str().to_string()).collect();
    let err = |m: String| Err(Exception::molt_err(Value::from(m)));
    if args.len() < min || args.len() > max {
        return err(format!("wrong # args: should be \"{name} {usage}\""));
    }
    handler(interp.context::<Session>(ctx), &args).or_else(err)
}

pub struct Script {
    interp: Interp,
    ctx: ContextID,
}

impl Script {
    pub fn new(grid: i32) -> Self {
        let mut interp = Interp::new();
        let ctx = interp.save_context(Session {
            grid,
            ..Default::default()
        });
        for c in COMMANDS {
            interp.add_context_command(c.0, dispatch, ctx);
        }
        Script { interp, ctx }
    }

    /// Run a script; `Ok` is the result text, `Err` the Tcl error message.
    pub fn eval(&mut self, src: &str) -> Result<String, String> {
        match self.interp.eval(src) {
            Ok(v) => Ok(v.as_str().to_string()),
            Err(e) => Err(e.value().as_str().to_string()),
        }
    }

    pub fn session(&mut self) -> &mut Session {
        self.interp.context::<Session>(self.ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> Script {
        let mut s = Script::new(20);
        s.eval("open tests/fixtures/RC4_sync_top.vhd").unwrap();
        s
    }

    fn names(s: &mut Script) -> Vec<String> {
        s.eval("instances")
            .unwrap()
            .split_whitespace()
            .map(String::from)
            .collect()
    }

    #[test]
    fn open_lists_instances_and_ports() {
        let mut s = Script::new(20);
        assert_eq!(s.eval("open tests/fixtures/radio_top.vhd").unwrap(), "11");
        assert_eq!(names(&mut s).len(), 11);
        let first = names(&mut s)[0].clone();
        assert!(
            s.eval(&format!("llength [ports {first}]"))
                .unwrap()
                .parse::<i32>()
                .unwrap()
                > 0
        );
        assert!(!s.eval("nets").unwrap().is_empty());
    }

    #[test]
    fn real_tcl_control_flow_drives_commands() {
        let mut s = script();
        s.eval("foreach i [instances] { color $i #ff0000 }")
            .unwrap();
        assert!(
            s.session()
                .diagram
                .instances
                .iter()
                .all(|i| i.color_override.as_deref() == Some("#ff0000"))
        );
        s.eval("proc nudge {n} { move $n 400 400 }; nudge [lindex [instances] 0]")
            .unwrap();
        assert_eq!(s.session().diagram.instances[0].x, 400);
    }

    #[test]
    fn edits_undo_and_redo_one_step_each() {
        let mut s = script();
        let first = names(&mut s)[0].clone();
        s.eval(&format!("rename {first} renamed")).unwrap();
        assert_eq!(names(&mut s)[0], "renamed");
        s.eval("undo").unwrap();
        assert_eq!(names(&mut s)[0], first);
        s.eval("redo").unwrap();
        assert_eq!(names(&mut s)[0], "renamed");
        assert!(
            s.eval("undo; undo")
                .unwrap_err()
                .contains("nothing to undo")
        );
    }

    #[test]
    fn errors_are_tcl_errors_and_leave_the_diagram_alone() {
        let mut s = script();
        let before = s.session().diagram.clone();
        assert!(
            s.eval("rename nosuch x")
                .unwrap_err()
                .contains("no such instance")
        );
        assert!(s.eval("move").unwrap_err().contains("wrong # args"));
        let first = names(&mut s)[0].clone();
        assert!(
            s.eval(&format!("move {first} a b"))
                .unwrap_err()
                .contains("integer")
        );
        assert!(
            s.eval(&format!("color {first} red"))
                .unwrap_err()
                .contains("not #rgb")
        );
        s.eval(&format!("lock {first}")).unwrap();
        assert!(
            s.eval(&format!("move {first} 0 0"))
                .unwrap_err()
                .contains("locked")
        );
        assert!(
            s.eval("catch {rename nosuch x} msg; set msg")
                .unwrap()
                .contains("no such")
        );
        s.eval(&format!("unlock {first}")).unwrap();
        s.eval("undo; undo").unwrap();
        assert_eq!(s.session().diagram, before);
    }

    #[test]
    fn group_collapse_ungroup_roundtrip() {
        let mut s = script();
        let n = names(&mut s);
        s.eval(&format!("group G {} {}", n[0], n[1])).unwrap();
        let after = names(&mut s);
        assert!(after.contains(&"G".into()) && !after.contains(&n[0]));
        s.eval("collapse G").unwrap();
        assert!(s.eval("info G").unwrap().contains("collapsed 1"));
        s.eval("expand G; ungroup G").unwrap();
        assert!(names(&mut s).contains(&n[0]));
        assert!(s.eval("ungroup nosuch").is_err());
        assert!(
            s.eval(&format!("ungroup {}", n[2]))
                .unwrap_err()
                .contains("not a group")
        );
    }

    #[test]
    fn hide_show_size_and_save_open_roundtrip() {
        let mut s = script();
        let n = names(&mut s)[0].clone();
        s.eval(&format!("size {n} 333 100")).unwrap();
        assert!(s.eval(&format!("info {n}")).unwrap().contains("width 340"));
        s.eval(&format!("hide {n}")).unwrap();
        assert!(s.eval(&format!("info {n}")).unwrap().contains("visible 0"));
        let p = std::env::temp_dir().join("hdl_diag_script_test.json");
        s.eval(&format!("save {}", p.display())).unwrap();
        let mut t = Script::new(20);
        t.eval(&format!("open {}", p.display())).unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(t.session().diagram, s.session().diagram);
    }

    #[test]
    fn export_writes_svg() {
        let mut s = script();
        let p = std::env::temp_dir().join("hdl_diag_script_test.svg");
        s.eval(&format!("export {}", p.display())).unwrap();
        let svg = std::fs::read_to_string(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert!(svg.starts_with("<svg") && svg.contains("<line "));
    }

    #[test]
    fn validate_and_interfaces_commands_run_and_undo() {
        let mut s = script();
        assert!(s.eval("validate").is_ok());
        assert!(s.eval("interfaces collapse").is_ok());
        assert!(s.eval("undo").is_ok(), "collapse is one undoable step");
        assert!(s.eval("interfaces bogus").is_err());
    }

    #[test]
    fn group_membership_commands() {
        let mut s = script();
        let names = s.eval("instances").unwrap();
        let mut it = names.split_whitespace();
        let (a, b) = (
            it.next().unwrap().to_string(),
            it.next().unwrap().to_string(),
        );
        s.eval(&format!("group G {a}")).unwrap();
        s.eval(&format!("addtogroup G {b}")).unwrap();
        assert!(s.eval("addtogroup G G").is_err());
        s.eval(&format!("release G {a}")).unwrap();
        assert!(s.eval(&format!("release G {a}")).is_err());
        s.eval("deletegroup G").unwrap();
        assert!(!s.eval("instances").unwrap().contains(&b));
        s.eval("undo").unwrap();
        assert!(s.eval("instances").unwrap().contains('G'));
    }
}
