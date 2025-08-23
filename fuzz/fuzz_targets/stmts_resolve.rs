#![no_main]

use grapl::{
    Resolve,
    resolve::{Config, Env},
};
use libfuzzer_sys::fuzz_target;
use rand::prelude::*;

#[path = "../../tests/test_helper.rs"]
mod test_helper;
use self::test_helper::*;

fuzz_target!(|data: &[u8]| {
    if let Ok(seed) = data.try_into() {
        let mut rng = StdRng::from_seed(seed);
        let stmts_max_len = rng.random_range(0..200);
        let depth = rng.random_range(0..25);
        let cweight = rng.random::<u32>() as usize;
        let dweight = rng.random::<u32>() as usize;
        let stmts = generate_stmts(25, stmts_max_len, depth, cweight, dweight);
        let config = Config::default().with_shadowing();
        let mut env = Env::new(&config);
        stmts.resolve(&mut env).ok();
    }
});
