//! Messages cross a private bus daemon, which validates their encoding:
//! calls, returns, errors and signals with nested containers.
#![cfg(target_os = "linux")]

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};

use aegle_dbus::{Connection, Kind, Message, Value};

/// A private `dbus-daemon` and its address, or `None` without one.
fn daemon() -> Option<(Child, String)> {
    let dir = std::env::temp_dir().join(format!("aegle-dbus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address"])
        .arg(format!("--address=unix:path={}/bus", dir.display()))
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut address = String::new();
    BufReader::new(child.stdout.take()?)
        .read_line(&mut address)
        .ok()?;
    Some((child, address.trim().to_owned()))
}

fn wait(incoming: &mut aegle_dbus::Incoming, test: impl Fn(&Message) -> bool) -> Message {
    loop {
        let message = incoming.recv().unwrap();
        if test(&message) {
            return message;
        }
    }
}

#[test]
fn messages_cross_a_bus() -> std::io::Result<()> {
    let Some((mut child, address)) = daemon() else {
        eprintln!("no dbus-daemon; skipped");
        return Ok(());
    };
    let timeout = Duration::from_secs(2);
    let (service, client) = (
        Connection::connect(&address, timeout)?,
        Connection::connect(&address, timeout)?,
    );
    assert!(service.name().starts_with(':'));
    let (mut served, mut received) = (service.incoming()?, client.incoming()?);
    client.add_match("type='signal',interface='org.example.Test'")?;

    let nested = vec![
        Value::str("a"),
        Value::dict([
            ("multiple", Value::Bool(true)),
            ("big", Value::U64(u64::MAX)),
            ("ratio", Value::F64(0.25)),
            ("names", Value::strings(["x", "y"])),
        ]),
        Value::Array(
            "(iiay)".into(),
            vec![Value::Struct(vec![
                Value::I32(-2),
                Value::I32(3),
                Value::Array("y".into(), vec![Value::Byte(7); 5]),
            ])],
        ),
        Value::Array("s".into(), vec![]),
        Value::Path("/org/example/Thing".into()),
    ];
    let call = Message::call(
        service.name(),
        "/org/example",
        "org.example.Test",
        "Echo",
        nested.clone(),
    );
    let serial = client.send(&call)?;
    let request = wait(&mut served, |m| m.is_call("org.example.Test", "Echo"));
    assert_eq!(request.body, nested);
    assert_eq!(request.sender, client.name());
    service.send(&request.reply(request.body.clone()))?;
    let reply = wait(&mut received, |m| m.reply_serial == serial);
    assert_eq!((reply.kind, reply.body), (Kind::Return, nested.clone()));

    let serial = client.send(&Message::call(
        service.name(),
        "/",
        "org.example.Test",
        "Fail",
        vec![],
    ))?;
    let request = wait(&mut served, |m| m.is_call("org.example.Test", "Fail"));
    service.send(&request.error("org.example.Error", "no"))?;
    let error = wait(&mut received, |m| m.reply_serial == serial);
    assert_eq!(
        (error.kind, error.error.as_str()),
        (Kind::Error, "org.example.Error")
    );

    let signal = Message::signal(
        "/org/example",
        "org.example.Test",
        "Changed",
        vec![Value::variant(Value::I64(-5))],
    );
    service.send(&signal)?;
    let seen = wait(&mut received, |m| {
        m.is_signal("org.example.Test", "Changed")
    });
    assert_eq!(seen.body[0].as_i64(), Some(-5));
    assert_eq!(seen.body, signal.body);
    // GLib marks calls with a zero Unix descriptor count; they still decode.
    // A read timeout turns a skipped call into a failure instead of a hang.
    served.stream().set_read_timeout(Some(timeout))?;
    if let Ok(gdbus) = Command::new("gdbus")
        .args([
            "call",
            "-t",
            "2",
            "--address",
            &address,
            "--dest",
            service.name(),
        ])
        .args([
            "--object-path",
            "/org/example",
            "--method",
            "org.example.Test.Ping",
            "x",
        ])
        .stdout(Stdio::piped())
        .spawn()
    {
        // gdbus introspects the object to type the arguments first.
        let introspect = wait(&mut served, |m| m.member == "Introspect");
        service.send(&introspect.error("org.freedesktop.DBus.Error.UnknownMethod", ""))?;
        let ping = wait(&mut served, |m| m.is_call("org.example.Test", "Ping"));
        assert_eq!(ping.body, [Value::str("x")]);
        service.send(&ping.reply(vec![Value::U32(7)]))?;
        let output = gdbus.wait_with_output()?;
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "(uint32 7,)"
        );
    }
    child.kill()?;
    let _ = std::fs::remove_dir_all(
        std::env::temp_dir().join(format!("aegle-dbus-{}", std::process::id())),
    );
    Ok(())
}
