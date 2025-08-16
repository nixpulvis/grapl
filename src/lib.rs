use chumsky::prelude::*;

#[derive(Debug, PartialEq, Eq)]
pub enum Expr<'src> {
    Node(&'src str),
    Connected(Vec<Expr<'src>>),
    Disconnected(Vec<Expr<'src>>),
}

pub fn parse<'src>() -> impl Parser<'src, &'src str, Expr<'src>> {
    recursive(|expr| {
        let ident = text::ascii::ident().padded().map(Expr::Node);

        let seq = expr
            .clone()
            .separated_by(just(","))
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

        let graph = connected.or(disconnected);

        ident.or(graph)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        const A: Expr<'static> = Expr::Node("A");
        const B: Expr<'static> = Expr::Node("B");
        const C: Expr<'static> = Expr::Node("C");
        const D: Expr<'static> = Expr::Node("D");

        assert!(parse().parse("").has_errors());
        assert_eq!(parse().parse("A").into_result(), Ok(A));
        assert_eq!(
            parse().parse("{A}").into_result(),
            Ok(Expr::Connected(vec![A]))
        );
        assert_eq!(
            parse().parse("[A,B,]").into_result(),
            Ok(Expr::Disconnected(vec![A, B]))
        );
        assert_eq!(
            parse().parse("[{A,B},[C, D]]").into_result(),
            Ok(Expr::Disconnected(vec![
                Expr::Connected(vec![A, B]),
                Expr::Disconnected(vec![C, D])
            ]))
        );
    }
}
