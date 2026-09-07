use assertr::prelude::*;

#[test]
fn dispatch_frontend_mutations_do_not_use_form_transports() {
    fn rust_sources(path: &std::path::Path, sources: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(path).expect("frontend source directory should be readable")
        {
            let path = entry
                .expect("frontend source entry should be readable")
                .path();
            if path.is_dir() {
                rust_sources(&path, sources);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }

    let frontend = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/frontend");
    let mut sources = Vec::new();
    rust_sources(&frontend, &mut sources);
    let forbidden = [
        ["<", "form"].concat(),
        ["Action", "Form"].concat(),
        ["request", "Submit("].concat(),
    ];
    let offenders = sources
        .into_iter()
        .filter_map(|path| {
            let source =
                std::fs::read_to_string(&path).expect("frontend Rust source should be readable");
            forbidden
                .iter()
                .any(|needle| source.contains(needle))
                .then_some(path)
        })
        .collect::<Vec<_>>();

    assert_that!(&(offenders.is_empty())).with_detail_message(format!("Dispatch frontend controls must use typed services, not HTML form elements: {offenders:?}")).is_true();
}
