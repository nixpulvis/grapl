use chumsky::container::Seq;
use grapl::{Expr, Node, Parse, Stmt};
use rand::distr::weighted::WeightedIndex;
use rand::seq::IteratorRandom;
use rand::{Rng, distr::Alphanumeric};

#[allow(unused)]
pub fn generate_node(max_len: usize) -> Node {
    let len = rand::rng().random_range(0..=max_len);
    let name = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect::<String>();
    Node::parse(&format!("N{name}"))
        .into_result()
        .expect("invalid node generated")
}

fn generate_delim_expr(
    left: char,
    right: char,
    node_max_len: usize,
    depth: usize,
    cweight: usize,
    dweight: usize,
) -> Expr {
    let len = rand::rng().random_range(0..=depth * 4);
    let seq = len
        .seq_iter()
        .fold(format!("{}", generate_node(node_max_len)), |s, _| {
            let inner = if depth == 0 {
                Expr::Node(generate_node(node_max_len))
            } else {
                generate_expr(node_max_len, depth - 1, cweight, dweight)
            };
            format!("{}, {}", s, inner)
        });
    let expr_string = format!("{left}{seq}{right}");
    Expr::parse(&expr_string)
        .into_result()
        .expect("invalid expr generated")
}

#[allow(unused)]
pub fn generate_expr(node_max_len: usize, depth: usize, cweight: usize, dweight: usize) -> Expr {
    let weights = [1, cweight, dweight];
    let dist = WeightedIndex::new(&weights).unwrap();
    let choice = rand::rng().sample(dist);
    match choice {
        0 => Expr::Node(generate_node(node_max_len)),
        1 => generate_delim_expr('{', '}', node_max_len, depth, cweight, dweight),
        _ => generate_delim_expr('[', ']', node_max_len, depth, cweight, dweight),
    }
}

#[allow(unused)]
pub fn generate_stmts(
    node_max_len: usize,
    stmts_max_len: usize,
    depth: usize,
    cweight: usize,
    dweight: usize,
) -> Vec<Stmt> {
    let len = rand::rng().random_range(0..=stmts_max_len);
    let mut stmts = vec![];
    for _ in (0..len) {
        let node = if rand::rng().random_bool(0.666) {
            generate_node(node_max_len)
        } else {
            stmts
                .iter()
                .choose(&mut rand::rng())
                .map_or(generate_node(node_max_len), |stmt| match stmt {
                    Stmt::Assign(node, _) => node.clone(),
                })
        };
        let stmt = Stmt::Assign(node, generate_expr(node_max_len, depth, cweight, dweight));
        stmts.push(stmt);
    }
    stmts
}
