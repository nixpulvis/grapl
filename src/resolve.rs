//! Replacement using assign statements.
//!
//! This module defines the needed structures to keep track of and properly
//! replace nodes which refer to previously bound nodes in [`Stmt::Assign`]
//! statements. There are some subtleties with respect to node shadowing and
//! recursion. See [`Config`] and [`Env`] for more information on how this is
//! handled.

use crate::{Expr, Node, Ret, Stmt};
use std::collections::HashMap;

/// Graph resolution configuration options.
#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    shadowing: bool,
    // TODO: This will probably start by looking something like this:
    // ```
    // Config { recursion: Recursion, ... }
    // struct RecursionConfig { limit: usize, ... }
    // struct Recursion { config: &RecursionConfig, depth: usize, ... }
    // struct Env(HashMap, Config, Recursion)
    // ```
    recursion: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // TODO: Is the right default?
            shadowing: false,
            recursion: false,
        }
    }
}

impl Config {
    /// Allow redefinition of nodes in assignment.
    ///
    /// ```grapl
    /// G = A
    /// G = B
    /// G => B
    /// ```
    pub fn with_shadowing(mut self) -> Self {
        self.shadowing = true;
        self
    }

    /// TODO
    pub fn with_recursion(mut self) -> Self {
        self.recursion = true;
        self
    }
}

#[derive(Debug)]
/// Errors that can occur during resolution.
pub enum Error {
    /// ```grapl
    /// G = A
    /// G = B
    /// ```
    Shadowing,
    /// ```grapl
    /// G = {G, B}
    /// ```
    Recursion,
}

/// Running resolution environment used to maintain state.
#[derive(Debug, PartialEq, Eq)]
pub struct Env<'cfg>(HashMap<Node, Expr>, &'cfg Config);

impl<'cfg> Env<'cfg> {
    /// Create a new empty resolution environment.
    pub fn new(config: &'cfg Config) -> Self {
        Env(HashMap::new(), config)
    }

    /// Returns the expression bound to the given node in this environment.
    pub fn lookup(&self, node: &Node) -> Expr {
        if let Some(expr) = self.0.get(node) {
            expr.clone()
        } else {
            Expr::Node(node.clone())
        }
    }

    /// Inserts the given node's expression into this environment.
    ///
    /// This function returns an error when it detects [`Error::Shadowing`] or
    /// [`Error::Recursion`] depending on if it's allowed by this environment's
    /// configuration.
    pub fn insert(&mut self, node: Node, expr: Expr) -> Result<(), Error> {
        if !self.1.shadowing && self.0.contains_key(&node) {
            Err(Error::Shadowing)
        } else if !self.1.recursion && expr.contains_node(&node) {
            Err(Error::Recursion)
        } else {
            self.0.insert(node, expr);
            Ok(())
        }
    }
}

/// Resolution of named graphs.
///
/// ```grapl
/// G = [A, B]
/// {X, G}
/// =>
/// {X, [A, B]}
/// ```
pub trait Resolve<'src>
where
    Self: Sized,
{
    type Output;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg>) -> Result<Self::Output, Error>;
}

impl<'src> Resolve<'src> for Expr {
    type Output = Self;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg>) -> Result<Self::Output, Error> {
        macro_rules! inner {
            ($exprs:expr, $variant:path) => {{
                let mut fresh = vec![];
                for expr in $exprs {
                    fresh.push(expr.resolve(env)?);
                }
                Ok($variant(fresh))
            }};
        }

        match self {
            Expr::Node(node) => Ok(env.lookup(node)),
            Expr::Connected(exprs) => inner!(exprs, Expr::Connected),
            Expr::Disconnected(exprs) => inner!(exprs, Expr::Disconnected),
        }
    }
}

impl<'src> Resolve<'src> for Stmt {
    type Output = Self;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg>) -> Result<Self::Output, Error> {
        match self {
            Stmt::Assign(node, expr) => {
                let resolved = expr.resolve(env)?;
                env.insert(node.clone(), resolved.clone())?;
                Ok(Stmt::Assign(node.clone(), resolved))
            }
        }
    }
}

impl<'src> Resolve<'src> for Vec<Stmt> {
    type Output = Self;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg>) -> Result<Self::Output, Error> {
        let mut fresh = vec![];
        for stmt in self {
            fresh.push(stmt.resolve(env)?);
        }
        Ok(fresh)
    }
}

impl<'src> Resolve<'src> for Ret {
    type Output = Expr;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg>) -> Result<Self::Output, Error> {
        self.0.resolve(env)?;
        self.1.resolve(env)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Expr, Node, Parse, Resolve, Ret, Stmt,
        resolve::{Config, Env},
    };
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_expr() {
        let config = Config::default();
        let mut env = Env::new(&config);
        env.insert(Node("A".into()), Expr::Node(Node("B".into())))
            .unwrap();

        assert_eq!(
            Expr::parse("A").unwrap().resolve(&mut env).unwrap(),
            Expr::parse("B").unwrap(),
        );
    }

    #[test]
    fn resolve_stmts() {
        let config = Config::default();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = [A, B]
                    G2 = {X, G1}
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = [A, B]
                    G2 = {X, [A, B]}
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    fn resolve_ret() {
        let config = Config::default();
        let mut env = Env::new(&config);

        assert_eq!(
            Ret::parse(
                r#"
                    G = A
                    A
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Expr::parse("A").unwrap(),
        );
    }

    #[test]
    fn resolve_shadowing() {
        let config = Config::default().with_shadowing();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = A
                    G1 = B
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = A
                    G1 = B
                    G2 = B
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    fn resolve_apparent_recursion_shadowing() {
        let config = Config::default().with_shadowing();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G2
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    #[ignore]
    fn resolve_recursion() {
        let config = Config::default().with_recursion();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G = {G, X}
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            // TODO: Handle multi-step resolution and proper recursion end
            // conditions.
            Vec::<Stmt>::parse(
                r#"
                    G = {{G..., X}, X}
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    #[ignore]
    fn resolve_mutual_recursion() {
        let config = Config::default().with_recursion();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = {G2, X}
                    G2 = {G1, Y}
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = {{G1..., Y}, X}
                    G2 = {{G2..., X}, Y}
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    #[ignore]
    fn resolve_direct_mutual_recursion() {
        let config = Config::default().with_recursion();
        let mut env = Env::new(&config);

        assert!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .is_err()
        );
    }

    #[test]
    #[ignore]
    fn resolve_direct_mutual_recursion_shadowing() {
        let config = Config::default().with_recursion().with_shadowing();
        let mut env = Env::new(&config);

        // This is going to eventually be an error in one way or another.
        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2...
                    G2 = G1...
                "#
            )
            .unwrap(),
        );
    }
}
