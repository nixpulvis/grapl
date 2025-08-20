use chumsky::container::Seq;
use grapl::{Expr, Node, Normalize, Parse};
use rand::distr::weighted::WeightedIndex;
use rand::{Rng, distr::Alphanumeric};

fn generate_node() -> Node {
    let name = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect::<String>();
    Node::parse(&format!("N{name}"))
        .into_result()
        .expect("invalid node generated")
}

fn generate_delim_expr(
    left: char,
    right: char,
    depth: usize,
    cweight: usize,
    dweight: usize,
) -> Expr {
    let inner = if depth == 0 {
        Expr::Node(generate_node())
    } else {
        generate_expr(depth - 1, cweight, dweight)
    };
    let len = rand::rng().random_range(0..100);
    let seq = len.seq_iter().fold(format!("{}", generate_node()), |s, _| {
        format!("{}, {}", s, inner)
    });
    let expr_string = format!("{left}{seq}{right}");
    Expr::parse(&expr_string)
        .into_result()
        .expect("invalid expr generated")
}

fn generate_expr(depth: usize, cweight: usize, dweight: usize) -> Expr {
    let weights = [1, cweight, dweight];
    let dist = WeightedIndex::new(&weights).unwrap();
    let choice = rand::rng().sample(dist);
    match choice {
        0 => Expr::Node(generate_node()),
        1 => generate_delim_expr('{', '}', depth, cweight, dweight),
        _ => generate_delim_expr('[', ']', depth, cweight, dweight),
    }
}

const ITERATIONS: usize = 100;
const RATIO_DEPTH: usize = 25;
const RATIO_DEEP_DEPTH: usize = 100;

#[test]
fn random_nodes_and_edges() {
    for _ in 0..ITERATIONS {
        let depth = rand::rng().random_range(0..25);
        let expr = generate_expr(depth, 10, 10);
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
        let expr = generate_expr(depth, cweight, dweight);
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
