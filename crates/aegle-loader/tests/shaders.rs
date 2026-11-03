//! The Shader element: a program file drawn with bound values, failures
//! with their place, and values checked like other properties.

use aegle_loader::{Data, Elements, Program};
use aegle_ui::{Result, Visit, scene::Command};

mod common;
use common::{names, ui};

fn program(source: &str) -> Result<Program> {
    Program::from_sources("main.aegle", &Elements::new(), &mut |_| Ok(source.into()))
}

/// The values of every program drawn.
fn drawn(ui: &aegle_ui::Ui) -> Result<Vec<[f32; 8]>> {
    ui.refresh()?;
    let mut values = Vec::new();
    ui.visit_scenes(|visit| {
        if let Visit::Scene { scene, .. } = visit {
            for command in scene.commands() {
                if let Command::Shader { values: v, .. } = command {
                    values.push(*v);
                }
            }
        }
        Ok(())
    })?;
    Ok(values)
}

#[test]
fn programs_draw_with_bound_values_and_report_failures() -> Result {
    let dir = std::env::temp_dir().join(format!("aegle-shaders-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let glow = dir.join("glow.wgsl");
    std::fs::write(
        &glow,
        "fn shade(uv: vec2f, size: vec2f) -> vec4f {\n    return vec4f(uv, values[0], 1.0);\n}\n",
    )?;
    let bad = dir.join("bad.wgsl");
    std::fs::write(
        &bad,
        "fn shade(uv: vec2f, size: vec2f) -> vec4f {\n    return nope;\n}\n",
    )?;
    let ui = ui()?;
    let source = format!(
        r#"Column {{
            state file: string = "{}"
            state level: float = 0.5
            state error: string = ""
            Shader {{ source: file; values: [level, 2]; width: 40dp; height: 20dp
                on failed {{ error = self.error }} }}
            Text {{ text: error }}
        }}"#,
        glow.display()
    );
    let view = program(&source)?.build(&ui.root())?;
    let mut expected = [0.0; 8];
    expected[..2].copy_from_slice(&[0.5, 2.0]);
    assert_eq!(drawn(&ui)?, [expected]);
    view.set("level", Data::Float(0.25))?;
    expected[0] = 0.25;
    assert_eq!(drawn(&ui)?, [expected]);

    view.set("file", Data::String(bad.to_str().unwrap().into()))?;
    ui.dispatch_callbacks()?;
    assert!(drawn(&ui)?.is_empty(), "a broken program draws nothing");
    let place = format!("{}:2:12: ", bad.display());
    assert!(
        names(&ui)?.iter().any(|text| text.starts_with(&place)),
        "{:?}",
        names(&ui)?
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn values_are_checked() {
    for values in ["[1, 2, 3, 4, 5, 6, 7, 8, 9]", "[\"a\"]", "3"] {
        let source = format!("Column {{ Shader {{ source: \"a.wgsl\"; values: {values} }} }}");
        assert!(program(&source).is_err(), "{values}");
    }
    assert!(program("Column { Shader { values: [] } }").is_ok());
}
