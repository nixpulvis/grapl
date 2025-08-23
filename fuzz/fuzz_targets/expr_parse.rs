#![no_main]

use grapl::{Expr, Parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = str::from_utf8(data) {
        if let Ok(expr) = Expr::parse(s).into_result() {
            expr.nodes();
            expr.edges();
        }
    }
});
