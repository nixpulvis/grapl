use grapl::Normalize;
use rand::Rng;

#[path = "test_helper.rs"]
mod test_helper;
use self::test_helper::*;

const ITERATIONS: usize = 100;
const RATIO_DEPTH: usize = 25;
const RATIO_DEEP_DEPTH: usize = 100;

#[test]
fn random_nodes_and_edges() {
    for _ in 0..ITERATIONS {
        let depth = rand::rng().random_range(0..25);
        let expr = generate_expr(25, depth, 10, 10);
        let normalized = expr.normalize();
        assert_eq!(expr.nodes(), normalized.nodes());
        assert_eq!(expr.edges(), normalized.edges());
    }
}

fn random_display_ratio(max_depth: usize, cweight: usize, dweight: usize) -> f64 {
    let mut len = 0;
    let mut norm_len = 0;
    for _ in 0..ITERATIONS {
        let depth = rand::rng().random_range(0..max_depth);
        let expr = generate_expr(25, depth, cweight, dweight);
        let normalized = expr.normalize();
        len += format!("{}", expr).len();
        norm_len += format!("{}", normalized).len();
    }
    len as f64 / norm_len as f64
}

#[test]
fn random_balanced_ratio() {
    let ratio = random_display_ratio(25, 25, RATIO_DEPTH);
    dbg!(ratio);
    assert!(ratio < 1.);
}

#[test]
fn random_mostly_connected_ratio() {
    let ratio = random_display_ratio(25, 10, RATIO_DEPTH);
    dbg!(ratio);
    assert!(ratio < 1.);
}

#[test]
fn random_mostly_disconnected_ratio() {
    let ratio = random_display_ratio(10, 25, RATIO_DEPTH);
    dbg!(ratio);
    assert!(ratio < 1.);
}

#[test]
fn random_balanced_deep_ratio() {
    let ratio = random_display_ratio(25, 25, RATIO_DEEP_DEPTH);
    dbg!(ratio);
    assert!(ratio < 1.);
}
