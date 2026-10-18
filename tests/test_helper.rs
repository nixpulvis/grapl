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
    tweight: usize,
) -> Expr {
    let len = rand::rng().random_range(0..=depth * 4);
    let seq = len
        .seq_iter()
        .fold(format!("{}", generate_node(node_max_len)), |s, _| {
            let inner = if depth == 0 {
                Expr::Node(generate_node(node_max_len))
            } else {
                generate_directed_expr(node_max_len, depth - 1, cweight, dweight, tweight)
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
    generate_directed_expr(node_max_len, depth, cweight, dweight, 0)
}

/// Like [`generate_expr`] but also generates directed `(...)` expressions with
/// the given weight.
#[allow(unused)]
pub fn generate_directed_expr(
    node_max_len: usize,
    depth: usize,
    cweight: usize,
    dweight: usize,
    tweight: usize,
) -> Expr {
    let weights = [1, cweight, dweight, tweight];
    let dist = WeightedIndex::new(&weights).unwrap();
    let choice = rand::rng().sample(dist);
    match choice {
        0 => Expr::Node(generate_node(node_max_len)),
        1 => generate_delim_expr('{', '}', node_max_len, depth, cweight, dweight, tweight),
        2 => generate_delim_expr('[', ']', node_max_len, depth, cweight, dweight, tweight),
        _ => generate_delim_expr('(', ')', node_max_len, depth, cweight, dweight, tweight),
    }
}

/// The edges of an expression computed directly from the meaning of each
/// form, without normalizing. `[]` adds nothing, `{}` adds every pair between
/// members both ways, and `()` adds every pair from earlier to later members.
#[allow(unused)]
pub fn naive_edges(expr: &Expr) -> Vec<(Node, Node)> {
    fn pairs(a: &Expr, b: &Expr, edges: &mut Vec<(Node, Node)>) {
        for x in a.nodes() {
            for y in b.nodes() {
                if x != y {
                    edges.push((x.clone(), y));
                }
            }
        }
    }
    let mut edges = vec![];
    match expr {
        Expr::Node(_) => {}
        Expr::Disconnected(exprs) => {
            for e in exprs {
                edges.append(&mut naive_edges(e));
            }
        }
        Expr::Connected(exprs) | Expr::Directed(exprs) => {
            for (i, a) in exprs.iter().enumerate() {
                edges.append(&mut naive_edges(a));
                for b in &exprs[i + 1..] {
                    pairs(a, b, &mut edges);
                    if let Expr::Connected(_) = expr {
                        pairs(b, a, &mut edges);
                    }
                }
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
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
