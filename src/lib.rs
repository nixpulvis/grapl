use chumsky::prelude::*;

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<'src>(&'src str);

impl<'src> Parse<'src> for Node<'src> {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        text::ascii::ident().padded().map(Node)
    }
}

impl<'src> std::fmt::Display for Node<'src> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Expressions describe a graph.
///
/// Fully connected:
/// ```grapl
/// { A, B }
/// ```
///
/// Fully disconnected:
/// ```grapl
///
/// ```
///
/// Basic nested expression:
/// ```grapl
/// { A, [B, C] } => [{A, B}, {A, C}]
/// ```
///
///
/// ```grapl
/// [A <-> B] => {A, B}
/// {A <!> B} => [A, B]
///
/// {A, B, C <!> D} => [{A, B, C}, {A, B, D}]
/// [A, B, C <-> D] => [A, B, {C, D}]
/// ```
///
/// ```grapl
/// {A, B} + {C, D} => {A, B, C, D}
/// {A, B} * {B, C} => {B}
/// {A, B, C} - {C, D} => {A, B}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<'src> {
    /// See [`Node`]
    Node(Node<'src>),
    /// {}
    FullyConnected(Vec<Expr<'src>>),
    /// []
    FullyDisconnected(Vec<Expr<'src>>),
    /// <->
    Connected(Box<Expr<'src>>, Box<Expr<'src>>),
    /// <!>
    Disconnected(Box<Expr<'src>>, Box<Expr<'src>>),
    /// +
    Union(Box<Expr<'src>>, Box<Expr<'src>>),
    /// *
    Intersection(Box<Expr<'src>>, Box<Expr<'src>>),
    /// -
    Subtraction(Box<Expr<'src>>, Box<Expr<'src>>),
}

impl<'src> Parse<'src> for Expr<'src> {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        recursive(|expr| {
            let node = Node::parser().map(Expr::Node);

            let seq = expr
                .clone()
                .separated_by(just(",").padded())
                .allow_trailing()
                .collect::<Vec<_>>();

            let fully_connected = seq
                .clone()
                .delimited_by(just('{'), just('}'))
                .map(Expr::FullyConnected);

            let fully_disconnected = seq
                .clone()
                .delimited_by(just('['), just(']'))
                .map(Expr::FullyDisconnected);

            let atom = choice((node.clone(), fully_connected, fully_disconnected));

            macro_rules! binary {
                ($token:literal, $node:path) => {
                    atom.clone()
                        .then(just($token).padded())
                        .then(expr.clone())
                        .map(|((a, _), b)| $node(Box::new(a), Box::new(b)))
                };
            }

            let connected = binary!("<->", Expr::Connected);
            let disconnected = binary!("<!>", Expr::Disconnected);
            let union = binary!("+", Expr::Union);
            let intersection = binary!("*", Expr::Intersection);
            let subtraction = binary!("-", Expr::Subtraction);

            // Order matters here.
            choice((
                connected,
                disconnected,
                union,
                intersection,
                subtraction,
                atom,
            ))
            .padded()
        })
    }
}

impl<'src> std::fmt::Display for Expr<'src> {
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
            Expr::FullyConnected(exprs) => write!(f, "{{{}}}", joined(&exprs)),
            Expr::FullyDisconnected(exprs) => write!(f, "[{}]", joined(&exprs)),
            _ => todo!(),
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
pub enum Stmt<'src> {
    Assign(Node<'src>, Expr<'src>),
}

impl<'src> Parse<'src> for Stmt<'src> {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Node::parser()
            .then(just("=").padded())
            .then(Expr::parser())
            .map(|((n, _), e)| Stmt::Assign(n, e))
    }
}

impl<'src> Parse<'src> for Vec<Stmt<'src>> {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Stmt::parser()
            .separated_by(text::whitespace())
            .collect::<Vec<_>>()
    }
}

impl<'src> std::fmt::Display for Stmt<'src> {
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
pub struct Ret<'src>(Vec<Stmt<'src>>, Expr<'src>);

impl<'src> Parse<'src> for Ret<'src> {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Vec::<Stmt>::parser()
            .then(Expr::parser())
            .map(|(s, e)| Ret(s, e))
    }
}

impl<'src> std::fmt::Display for Ret<'src> {
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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! node {
        ($($id:ident),*) => {
            $(const $id: Node<'static> = Node(stringify!($id));)*
        };
    }

    #[test]
    fn parse_node() {
        node!(A);

        assert!(Node::parse("").has_errors());
        assert!(Node::parse("1").has_errors());
        assert_eq!(Node::parse("A").into_result(), Ok(A));
    }

    #[test]
    fn display_node() {
        assert_eq!(Node::parse("A").unwrap().to_string(), "A");
        assert_eq!(Node::parse("  G ").unwrap().to_string(), "G");
    }

    macro_rules! enode {
        ($($id:ident),*) => {
            $(const $id: Expr<'static> = Expr::Node(Node(stringify!($id)));)*
        };
    }

    #[test]
    fn parse_expr() {
        enode!(A, B, C, D);

        assert_eq!(
            Expr::parse("{}").into_result(),
            Ok(Expr::FullyConnected(vec![]))
        );
        assert_eq!(
            Expr::parse("[]").into_result(),
            Ok(Expr::FullyDisconnected(vec![]))
        );
        assert_eq!(
            Expr::parse("[A,  B,  ]").into_result(),
            Ok(Expr::FullyDisconnected(vec![A, B]))
        );
        assert_eq!(
            Expr::parse("{  A }").into_result(),
            Ok(Expr::FullyConnected(vec![A]))
        );
        assert_eq!(
            Expr::parse("[A,  B,  ]").into_result(),
            Ok(Expr::FullyDisconnected(vec![A, B]))
        );
        assert_eq!(
            Expr::parser()
                .parse(
                    r#"
                        {A, [B, C]}
                "#
                )
                .into_result(),
            Ok(Expr::FullyConnected(vec![
                A,
                Expr::FullyDisconnected(vec![B, C])
            ]))
        );
        assert_eq!(
            Expr::parse("[{A,B},[C, D]]").into_result(),
            Ok(Expr::FullyDisconnected(vec![
                Expr::FullyConnected(vec![A, B]),
                Expr::FullyDisconnected(vec![C, D])
            ]))
        );
        assert_eq!(
            Expr::parse("{{A, B}, [C, D]}").into_result(),
            Ok(Expr::FullyConnected(vec![
                Expr::FullyConnected(vec![A, B]),
                Expr::FullyDisconnected(vec![C, D])
            ]))
        );
        assert!(!Expr::parse("A  +   B").has_errors());
        assert!(!Expr::parse("{A,B}+{B,C}").has_errors());
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
        node!(G, H, G1, G2);
        enode!(A, B, C, D);

        assert!(Stmt::parse("").has_errors());
        assert_eq!(Vec::<Stmt>::parse("").into_result(), Ok(vec![]),);
        assert_eq!(
            Stmt::parse("G = {A, B}").into_result(),
            Ok(Stmt::Assign(G, Expr::FullyConnected(vec![A, B]))),
        );
        assert_eq!(
            Vec::<Stmt>::parse("G = {A, B}H = [C, D]").into_result(),
            Ok(vec![
                Stmt::Assign(G, Expr::FullyConnected(vec![A, B])),
                Stmt::Assign(H, Expr::FullyDisconnected(vec![C, D])),
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
                Stmt::Assign(G1, Expr::FullyConnected(vec![A, B])),
                Stmt::Assign(
                    G2,
                    Expr::FullyConnected(vec![Expr::FullyDisconnected(vec![Expr::Node(G1), C]), D])
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
        node!(G);
        enode!(A, B, C, D);

        assert_eq!(
            Ret::parse("{A, [C, D]}").into_result(),
            Ok(Ret(
                vec![],
                Expr::FullyConnected(vec![A, Expr::FullyDisconnected(vec![C, D])]),
            ))
        );

        assert_eq!(
            Ret::parse("G = {A, [C, D]} {G, B}").into_result(),
            Ok(Ret(
                vec![Stmt::Assign(
                    G,
                    Expr::FullyConnected(vec![A, Expr::FullyDisconnected(vec![C, D])])
                )],
                Expr::FullyConnected(vec![Expr::Node(G), B]),
            ))
        );

        assert_eq!(
            Expr::parse(
                r#"
                    {G, [C, D]}
                "#
            )
            .into_result(),
            Ok(Expr::FullyConnected(vec![
                Expr::Node(G),
                Expr::FullyDisconnected(vec![C, D])
            ]))
        );

        assert_eq!(
            Ret::parse(
                r#"
                    G = {A, B}

                    {G, [C, D]}
                "#
            )
            .into_result(),
            Ok(Ret(
                vec![Stmt::Assign(G, Expr::FullyConnected(vec![A, B]))],
                Expr::FullyConnected(vec![Expr::Node(G), Expr::FullyDisconnected(vec![C, D])]),
            ))
        );
    }

    #[test]
    fn display_ret() {
        assert_eq!(Ret::parse("  G=A B").unwrap().to_string(), "G = A\nB")
    }
}
