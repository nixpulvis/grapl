#![no_main]

use grapl::Normalize;
use libfuzzer_sys::fuzz_target;
use rand::prelude::*;

#[path = "../../tests/test_helper.rs"]
mod test_helper;
use self::test_helper::*;

fuzz_target!(|data: &[u8]| {
    if let Ok(seed) = data.try_into() {
        let mut rng = StdRng::from_seed(seed);
        let depth = rng.random_range(0..25);
        let cweight = rng.random_range(1..100);
        let dweight = rng.random_range(1..100);
        let expr = generate_expr(25, depth, cweight, dweight);
        expr.normalize();
    }
});
