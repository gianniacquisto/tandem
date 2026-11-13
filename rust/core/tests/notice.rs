//! The third-party notice check the old `app.yml` carried. `app/` and its Node
//! generator are deleted, but the binary still embeds vendored third-party
//! assets, so the notice must keep naming their upstreams. The subject moved;
//! the check must not disappear with it.

use std::path::Path;

#[test]
fn the_vendored_third_party_assets_are_named_in_the_notice() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let notice = std::fs::read_to_string(core.join("../../NOTICE.txt"))
        .expect("NOTICE.txt sits at the repository root");
    for (asset, upstream) in [("htmx.min.js", "htmx"), ("sse.js", "htmx-ext-sse")] {
        assert!(
            core.join("src/serve/static").join(asset).is_file(),
            "{asset} is vendored next to the routes that serve it"
        );
        assert!(
            notice.contains(upstream),
            "NOTICE.txt names {upstream}, the upstream of the vendored {asset}"
        );
    }
}
