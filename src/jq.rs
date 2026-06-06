//! Built-in jq filtering via the `jaq` crates, so `--jq` works without the user
//! having `jq` installed. Compiles the program once and runs it over one input.

use serde_json::Value;

pub fn run(input: &Value, program: &str) -> Result<Vec<Value>, String> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{Compiler, Ctx, Native, RcIter};
    use jaq_json::Val;

    let arena = Arena::default();
    let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
    let modules = loader
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|_| format!("invalid jq program: {program}"))?;

    let filter = Compiler::<_, Native<Val>>::default()
        .with_funs(jaq_std::funs().chain(jaq_json::funs()))
        .compile(modules)
        .map_err(|_| format!("could not compile jq program: {program}"))?;

    let inputs = RcIter::new(core::iter::empty());
    let ctx = Ctx::new([], &inputs);

    let mut out = Vec::new();
    for result in filter.run((ctx, Val::from(input.clone()))) {
        match result {
            Ok(v) => out.push(Value::from(v)),
            Err(e) => return Err(format!("{e}")),
        }
    }
    Ok(out)
}
