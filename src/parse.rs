//! Parsing text into syntax.
//!
//! Every syntax element implements [`Parse`]. Statements are separated by
//! whitespace and a [`Program`] is any number of statements followed by one
//! expression.

use crate::{Expr, Node, Program, Stmt};
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

impl<'src> Parse<'src> for Node {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        text::ascii::ident()
            .padded()
            .map(|t: &str| Node(t.to_string()))
    }
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

            let directed = seq
                .clone()
                .delimited_by(just('('), just(')'))
                .map(Expr::Directed);

            let atom = choice((node, connected, disconnected, directed)).padded();

            // `A -> B -> C` is sugar for the path `[(A, B), (B, C)]`.
            atom.separated_by(just("->").padded())
                .at_least(1)
                .collect::<Vec<_>>()
                .map(Expr::path)
        })
    }
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

impl<'src> Parse<'src> for Program {
    fn parser() -> impl Parser<'src, &'src str, Self> + Clone {
        Vec::<Stmt>::parser()
            .then(Expr::parser())
            .map(|(s, e)| Program::new(s, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_node() {
        assert!(Node::parse("").has_errors());
        assert!(Node::parse("1").has_errors());
        assert_eq!(Node::parse("A").into_result(), Ok(node!(A)));
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
    fn parse_directed_expr() {
        assert_eq!(
            Expr::parse("(A, B)").into_result(),
            Ok(Expr::Directed(vec![enode!(A), enode!(B)]))
        );
        assert_eq!(
            Expr::parse("{A, (B, [C, D])}").into_result(),
            Ok(Expr::Connected(vec![
                enode!(A),
                Expr::Directed(vec![
                    enode!(B),
                    Expr::Disconnected(vec![enode!(C), enode!(D)])
                ])
            ]))
        );
        // Arrows are sugar for paths.
        assert_eq!(
            Expr::parse("A -> B").into_result(),
            Ok(Expr::Directed(vec![enode!(A), enode!(B)]))
        );
        assert_eq!(
            Expr::parse("A -> B -> C").into_result(),
            Ok(Expr::Disconnected(vec![
                Expr::Directed(vec![enode!(A), enode!(B)]),
                Expr::Directed(vec![enode!(B), enode!(C)]),
            ]))
        );
        assert_eq!(
            Expr::parse("{A -> B, C}").into_result(),
            Ok(Expr::Connected(vec![
                Expr::Directed(vec![enode!(A), enode!(B)]),
                enode!(C),
            ]))
        );
        assert_eq!(
            Expr::parse("{A, B} -> C").into_result(),
            Ok(Expr::Directed(vec![
                Expr::Connected(vec![enode!(A), enode!(B)]),
                enode!(C),
            ]))
        );
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
    fn parse_ret() {
        assert_eq!(
            Program::parse("{A, [C, D]}").into_result(),
            Ok(Program::new(
                vec![],
                Expr::Connected(vec![
                    enode!(A),
                    Expr::Disconnected(vec![enode!(C), enode!(D)])
                ]),
            ))
        );

        assert_eq!(
            Program::parse("G = {A, [C, D]} {G, B}").into_result(),
            Ok(Program::new(
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
            Program::parser()
                .parse(
                    r#"
                        G = {A, B}

                        {G, [C, D]}
                "#
                )
                .into_result(),
            Ok(Program::new(
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
}
