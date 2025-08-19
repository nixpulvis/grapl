use chumsky::prelude::*;
use itertools::Itertools;
#[cfg(feature = "petgraph")]
use petgraph::Graph;
use std::hash::Hash;

/// Parsing for syntax elements.
///
/// Use [`Parse::parser`] only if you're extending the parser in some way. You
/// will need to depend on [`chumsky::Parser`] directly as well. Otherwise, just
/// use [`Parse::parse`] to get a syntax element.
pub trait Parse<'src>
where
    Self: Sized,
{
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone;

    fn parse(input: &'src str) -> ParseResult<Self, EmptyErr> {
        Self::parser().parse(input)
    }
}

/// Nodes used as base indentifiers or to refer to other graphs.
///
/// Examples of nodes: `A`, `a`, `G1`...
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Node(String);

impl<'src> Parse<'src> for Node {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        text::ascii::ident()
            .padded()
            .map(|t: &str| Node(t.to_string()))
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Expressions describe a graph.
///
/// ```grapl
/// { A, B }
/// { A, [B, C] }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Node(Node),
    Connected(Vec<Expr>),
    Disconnected(Vec<Expr>),
}

impl<'src> Parse<'src> for Expr {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        recursive(|expr| {
            let node = Node::parser().map(Expr::Node);

            let seq = expr
                .clone()
                .separated_by(just(",").padded())
                .allow_trailing()
                .collect::<Vec<_>>();

            let connected = seq
                .clone()
                .delimited_by(just('{'), just('}'))
                .map(Expr::Connected);

            let disconnected = seq
                .clone()
                .delimited_by(just('['), just(']'))
                .map(Expr::Disconnected);

            choice((node, connected, disconnected)).padded()
        })
    }
}

impl<'src> Expr {
    pub fn nodes(&self) -> Vec<Node> {
        match self {
            Expr::Node(node) => vec![node.clone()],
            Expr::Connected(exprs) | Expr::Disconnected(exprs) => exprs
                .iter()
                .fold(vec![], |mut v, e| {
                    v.append(&mut e.nodes());
                    v
                })
                .into_iter()
                .sorted()
                .dedup()
                .collect(),
        }
    }

    pub fn edges(&self) -> Vec<(Node, Node)> {
        match self.normalize() {
            Self::Node(_) => vec![],
            // TODO: directed vs undirected...
            e @ Self::Connected(_) => e
                .nodes()
                .iter()
                .cartesian_product(e.nodes().iter())
                .map(|(a, b)| (a.clone(), b.clone()))
                .filter(|(a, b)| a != b)
                .sorted()
                .dedup()
                .collect(),
            Self::Disconnected(exprs) => {
                let mut edges = vec![];
                for expr in exprs {
                    edges.append(&mut expr.edges());
                }
                edges.into_iter().sorted().dedup().collect()
            }
        }
    }

    pub fn contains_node(&self, node: &Node) -> bool {
        match self {
            Expr::Node(n) => node == n,
            Expr::Connected(exprs) | Expr::Disconnected(exprs) => {
                exprs.iter().any(|e| e.contains_node(node))
            }
        }
    }
}

#[cfg(feature = "petgraph")]
impl Into<Graph<Node, ()>> for Expr {
    fn into(self) -> Graph<Node, ()> {
        let mut graph: Graph<Node, _> = Graph::new();
        for node in self.nodes() {
            graph.add_node(node);
        }
        for (a, b) in self.edges() {
            let ia = graph
                .node_indices()
                .find(|idx| a == *graph.node_weight(*idx).unwrap())
                .unwrap();
            let ib = graph
                .node_indices()
                .find(|idx| b == *graph.node_weight(*idx).unwrap())
                .unwrap();
            graph.add_edge(ia, ib, ());
        }
        graph
    }
}

impl<'src> std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let joined = |exprs: &[Expr]| {
            exprs
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self.normalize() {
            Expr::Node(node) => write!(f, "{}", node),
            Expr::Connected(exprs) => write!(f, "{{{}}}", joined(&exprs)),
            Expr::Disconnected(exprs) => write!(f, "[{}]", joined(&exprs)),
        }
    }
}

/// A statement is part of a sequence used to resolve other statements.
///
/// ```grapl
/// G1 = {A, B}
/// G2 = [C, D]
/// G  = {G1, G2}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Assign(Node, Expr),
}

impl<'src> Parse<'src> for Stmt {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Node::parser()
            .then(just("=").padded())
            .then(Expr::parser())
            .map(|((n, _), e)| Stmt::Assign(n, e))
    }
}

impl<'src> Parse<'src> for Vec<Stmt> {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Stmt::parser()
            .separated_by(text::whitespace())
            .collect::<Vec<_>>()
    }
}

impl<'src> std::fmt::Display for Stmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.normalize() {
            Stmt::Assign(node, expr) => write!(f, "{} = {}", node, expr),
        }
    }
}

/// Returns are a sequence of statements followed by a final graph expression.
///
/// ```grapl
/// G = {A, B}
/// {G, [C, D]}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ret(Vec<Stmt>, Expr);

impl<'src> Parse<'src> for Ret {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Vec::<Stmt>::parser()
            .then(Expr::parser())
            .map(|(s, e)| Ret(s, e))
    }
}

impl<'src> std::fmt::Display for Ret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let joined = |exprs: &[Stmt]| {
            exprs
                .iter()
                .map(|e| e.normalize().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        };
        write!(f, "{}\n{}", joined(&self.0), self.1.normalize())
    }
}

mod normal;
pub use self::normal::Normalize;

pub mod resolve;
pub use self::resolve::Resolve;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    macro_rules! node {
        ($id:ident) => {
            Node(stringify!($id).into())
        };
    }

    #[test]
    fn parse_node() {
        assert!(Node::parse("").has_errors());
        assert!(Node::parse("1").has_errors());
        assert_eq!(Node::parse("A").into_result(), Ok(node!(A)));
    }

    #[test]
    fn display_node() {
        assert_eq!(Node::parse("A").unwrap().to_string(), "A");
        assert_eq!(Node::parse("  G ").unwrap().to_string(), "G");
    }

    macro_rules! enode {
        ($id:ident) => {
            Expr::Node(Node(stringify!($id).into()))
        };
    }

    #[test]
    fn parse_expr() {
        assert_eq!(Expr::parse("{}").into_result(), Ok(Expr::Connected(vec![])));
        assert_eq!(
            Expr::parse("[]").into_result(),
            Ok(Expr::Disconnected(vec![]))
        );
        assert_eq!(
            Expr::parse("{  A }").into_result(),
            Ok(Expr::Connected(vec![enode!(A)]))
        );
        assert_eq!(
            Expr::parse("[A,  B,  ]").into_result(),
            Ok(Expr::Disconnected(vec![enode!(A), enode!(B)]))
        );
        assert_eq!(
            Expr::parser()
                .parse(
                    r#"
                        {A, [B, C]}
                "#
                )
                .into_result(),
            Ok(Expr::Connected(vec![
                enode!(A),
                Expr::Disconnected(vec![enode!(B), enode!(C)])
            ]))
        );
        assert_eq!(
            Expr::parse("[{A,B},[C, D]]").into_result(),
            Ok(Expr::Disconnected(vec![
                Expr::Connected(vec![enode!(A), enode!(B)]),
                Expr::Disconnected(vec![enode!(C), enode!(D)])
            ]))
        );
        assert_eq!(
            Expr::parse("{{A, B}, [C, D]}").into_result(),
            Ok(Expr::Connected(vec![
                Expr::Connected(vec![enode!(A), enode!(B)]),
                Expr::Disconnected(vec![enode!(C), enode!(D)])
            ]))
        )
    }

    #[test]
    fn nodes_expr() {
        assert_eq!(
            Expr::parser().parse("{A, [B, C], D}").unwrap().nodes(),
            vec![node!(A), node!(B), node!(C), node!(D)]
        );
        assert_eq!(
            Expr::parser()
                .parse("[{A,B},{B,C},{C,D},{D,A}]")
                .unwrap()
                .nodes(),
            vec![node!(A), node!(B), node!(C), node!(D)]
        );
    }

    #[test]
    fn edges_expr() {
        for edge in Expr::parser().parse("{A, [B, C], D}").unwrap().edges() {
            println!("({}, {})", edge.0, edge.1);
        }
        assert_eq!(
            Expr::parser().parse("{A, [B, C], D}").unwrap().edges(),
            vec![
                (node!(A), node!(B)),
                (node!(A), node!(C)),
                (node!(A), node!(D)),
                (node!(B), node!(A)),
                (node!(B), node!(D)),
                (node!(C), node!(A)),
                (node!(C), node!(D)),
                (node!(D), node!(A)),
                (node!(D), node!(B)),
                (node!(D), node!(C)),
            ]
        );
    }

    #[test]
    fn contains_node_expr() {
        assert!(
            Expr::parser()
                .parse("{A, {B, [C, D]}, {E, F}}")
                .unwrap()
                .contains_node(&node!(C))
        )
    }

    #[test]
    fn display_expr() {
        assert_eq!(
            Expr::parser()
                .parse("{  A, {B  ,  [C,D]}  }")
                .unwrap()
                .to_string(),
            "[{A, B, C}, {A, B, D}]"
        )
    }

    #[test]
    fn parse_stmt() {
        assert!(Stmt::parse("").has_errors());
        assert_eq!(Vec::<Stmt>::parse("").into_result(), Ok(vec![]),);
        assert_eq!(
            Stmt::parse("G = {A, B}").into_result(),
            Ok(Stmt::Assign(
                node!(G),
                Expr::Connected(vec![enode!(A), enode!(B)])
            )),
        );
        assert_eq!(
            Vec::<Stmt>::parse("G = {A, B}H = [C, D]").into_result(),
            Ok(vec![
                Stmt::Assign(node!(G), Expr::Connected(vec![enode!(A), enode!(B)])),
                Stmt::Assign(node!(H), Expr::Disconnected(vec![enode!(C), enode!(D)])),
            ]),
        );
        assert_eq!(
            Vec::<Stmt>::parser()
                .parse(
                    r#"
                        G1 = {A, B}

                        G2 = {[G1, C], D}
                "#
                )
                .into_result(),
            Ok(vec![
                Stmt::Assign(node!(G1), Expr::Connected(vec![enode!(A), enode!(B)])),
                Stmt::Assign(
                    node!(G2),
                    Expr::Connected(vec![
                        Expr::Disconnected(vec![enode!(G1), enode!(C)]),
                        enode!(D)
                    ])
                ),
            ]),
        );
    }

    #[test]
    fn display_stmt() {
        assert_eq!(
            Stmt::parse("  G={A,[B,C]}").unwrap().to_string(),
            "G = [{A, B}, {A, C}]"
        )
    }

    #[test]
    fn parse_ret() {
        assert_eq!(
            Ret::parse("{A, [C, D]}").into_result(),
            Ok(Ret(
                vec![],
                Expr::Connected(vec![
                    enode!(A),
                    Expr::Disconnected(vec![enode!(C), enode!(D)])
                ]),
            ))
        );

        assert_eq!(
            Ret::parse("G = {A, [C, D]} {G, B}").into_result(),
            Ok(Ret(
                vec![Stmt::Assign(
                    node!(G),
                    Expr::Connected(vec![
                        enode!(A),
                        Expr::Disconnected(vec![enode!(C), enode!(D)])
                    ])
                )],
                Expr::Connected(vec![enode!(G), enode!(B)]),
            ))
        );

        assert_eq!(
            Ret::parser()
                .parse(
                    r#"
                        G = {A, B}

                        {G, [C, D]}
                "#
                )
                .into_result(),
            Ok(Ret(
                vec![Stmt::Assign(
                    node!(G),
                    Expr::Connected(vec![enode!(A), enode!(B)])
                )],
                Expr::Connected(vec![
                    enode!(G),
                    Expr::Disconnected(vec![enode!(C), enode!(D)])
                ]),
            ))
        );
    }

    #[test]
    fn display_ret() {
        assert_eq!(Ret::parse("  G=A B").unwrap().to_string(), "G = A\nB")
    }
}
