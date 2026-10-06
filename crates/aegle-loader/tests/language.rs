//! Records, keys, slots, component events, locals and host actions in running views.
use aegle_loader::{Data, Limits, Program, markup::Type};
use aegle_ui::Result;
use std::{cell::RefCell, rc::Rc};

mod common;
use common::{click, names, ui};

const MAIN: &str = r##"record Task { id: int; title: string; done: bool }
component Card(title: string) {
    event closed(int)
    state taps: int = 0
    Column {
        Text { text: title + "#" + str(taps) }
        slot
        Button { text: "close " + title; on clicked { taps += 1; let n = taps * 10; host.note(title, n); emit closed(n) } }
    }
}
Column {
    state tasks: list<Task> = [Task(1, "a", false), Task(2, "b", true)]
    state log: string = ""
    state total: int = 0
    Text { text: "log:" + log }
    Text { text: "total:" + str(total) }
    for task in tasks key task.id {
        Card {
            title: task.title
            on closed(n) { total += n; log += task.title }
            Text { text: "extra " + task.title + str(task.done) }
        }
    }
    Button { text: "rename"; on clicked { tasks = [Task(1, "a2", false), Task(2, "b", true)] } }
    Button { text: "add"; on clicked { tasks += [Task(3, "c", false)] } }
}"##;

type Notes = Rc<RefCell<Vec<(String, i64)>>>;

fn compile(main: &str) -> Result<(Program, Notes)> {
    let program = Program::from_sources("main.aegle", &mut |path| match path {
        "main.aegle" => Ok(main.into()),
        _ => Err("missing".into()),
    })?;
    let notes = Notes::default();
    let sink = notes.clone();
    program.action("note", &[Type::String, Type::Int], move |arguments| {
        let [Data::String(title), Data::Int(n)] = arguments else {
            unreachable!("validated argument types")
        };
        sink.borrow_mut().push((title.to_string(), *n));
        Ok(())
    });
    Ok((program, notes))
}

#[test]
fn records_slots_events_locals_and_host_actions_drive_the_interface() -> Result {
    let ui = ui()?;
    let (program, notes) = compile(MAIN)?;
    let view = program.build(&ui.root())?;
    assert_eq!(
        names(&ui)?,
        [
            "log:",
            "total:0",
            "a#0",
            "extra afalse",
            "close a",
            "b#0",
            "extra btrue",
            "close b",
            "rename",
            "add"
        ]
    );
    click(&ui, "close a")?; // let, host action, emit with a value, caller handler.
    assert_eq!(*notes.borrow(), [("a".to_owned(), 10)]);
    assert_eq!(names(&ui)?[..3], ["log:a", "total:10", "a#1"]);
    click(&ui, "close a")?;
    click(&ui, "close b")?;
    assert_eq!(names(&ui)?[..2], ["log:aab", "total:40"]);
    // Same key, same value: the row and its state stay. Same key, new value: rebuilt.
    click(&ui, "rename")?;
    let after = names(&ui)?;
    assert_eq!(after[2..6], ["a2#0", "extra a2false", "close a2", "b#1"]);
    click(&ui, "add")?;
    assert!(names(&ui)?.contains(&"c#0".to_owned()));
    // Rust reads and writes record lists as Data.
    let tasks = view.state::<Data>("tasks").unwrap();
    let Data::List(items) = tasks.get() else {
        panic!("a list")
    };
    assert_eq!(items.len(), 3);
    let record = |id: i64, title: &str, done: bool| {
        Data::Record(
            [Data::Int(id), Data::String(title.into()), Data::Bool(done)]
                .into_iter()
                .collect(),
        )
    };
    tasks.set(Data::List([record(7, "z", true)].into_iter().collect()))?;
    assert_eq!(names(&ui)?[3..5], ["extra ztrue", "close z"]);
    assert!(
        tasks
            .set(Data::List([Data::Int(1)].into_iter().collect()))
            .is_err()
    );
    assert!(
        tasks
            .set(Data::List(
                [record(1, "dup", false), record(1, "dup", false)]
                    .into_iter()
                    .collect(),
            ))
            .is_err()
    );
    Ok(())
}

#[test]
fn programs_check_actions_limits_and_name_the_failing_file() -> Result {
    let ui = ui()?;
    // An unregistered or mistyped action stops the build before anything mounts.
    let bare = Program::from_sources("main.aegle", &mut |_| {
        Ok(r#"Column { Button { text: "go"; on clicked { host.go(1) } } }"#.into())
    })?;
    let error = bare
        .build(&ui.root())
        .err()
        .expect("unregistered")
        .to_string();
    assert!(error.contains("`go` is not registered"), "{error}");
    bare.action("go", &[Type::String], |_| Ok(()));
    assert!(
        bare.build(&ui.root())
            .err()
            .expect("mistyped")
            .to_string()
            .contains("takes")
    );
    assert!(names(&ui)?.is_empty());
    bare.action("go", &[Type::Int], |_| Ok(()));
    bare.build(&ui.root())?;

    // The statement limit stops a handler and keeps earlier assignments.
    let ui = common::ui()?;
    let (program, _) = compile(
        r#"Column {
            state n: int = 0
            Button { text: "go"; on clicked { n += 1; n += 1; n += 1; n += 1 } }
            Text { text: str(n) }
        }"#,
    )?;
    program.set_limits(Limits {
        steps: 3,
        ..Limits::default()
    });
    let view = program.build(&ui.root())?;
    assert!(click(&ui, "go").is_err());
    assert_eq!(view.get("n"), Some(Data::Int(3)));
    // A list over the row limit leaves the block unchanged.
    let (program, _) = compile(
        r#"Column { state xs: list<int> = [1, 2]; for x in xs { Text { text: "row" + str(x) } } }"#,
    )?;
    program.set_limits(Limits {
        rows: 2,
        ..Limits::default()
    });
    let view = program.build(&ui.root())?;
    let error = view
        .set("xs", Data::List([1, 2, 3].map(Data::Int).into()))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("main.aegle") && error.contains("row limit"),
        "{error}"
    );
    // The state took the value, but the block keeps its two rows.
    assert!(!names(&ui)?.contains(&"row3".to_owned()));
    assert!(names(&ui)?.ends_with(&["row1".to_owned(), "row2".to_owned()]));
    Ok(())
}
