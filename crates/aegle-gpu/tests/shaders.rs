//! Custom fragment programs check like renderers compile them, and errors
//! point into the program.
#![cfg(feature = "shaders")]

use aegle_gpu::check_shader;

#[test]
fn programs_check_and_errors_point_into_them() {
    let glow = "fn shade(uv: vec2f, size: vec2f) -> vec4f {\n\
                \x20   let pulse = 0.5 + 0.5 * sin(time * values[0]);\n\
                \x20   return vec4f(uv, pulse, 1.0);\n\
                }\n";
    check_shader(glow).unwrap();

    let typo = "fn shade(uv: vec2f, size: vec2f) -> vec4f {\n    return vec4f(uv, nope, 1.0);\n}\n";
    let error = check_shader(typo).unwrap_err();
    assert_eq!(error.location.map(|(line, _)| line), Some(2), "{error}");
    assert!(error.to_string().starts_with("2:"), "{error}");

    let wrong = "fn shade(uv: vec2f) -> vec4f { return vec4f(uv, 0.0, 1.0); }";
    assert!(check_shader(wrong).is_err());
    assert!(check_shader("").is_err(), "a program defines shade");
}
