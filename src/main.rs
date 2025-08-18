use chumsky::Parser;
use grapl::resolve::{Config, Env};
use grapl::{Expr, Normalize, Parse, Resolve, Stmt};
use microxdg::{Xdg, XdgError};
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::{DefaultEditor, Editor};
use std::fs;
use std::path::PathBuf;

fn main() -> rustyline::Result<()> {
    let mut rl = DefaultEditor::new()?;
    load_history(&mut rl);

    let config = Config::default().with_shadowing();
    let mut env = Env::new(&config);

    loop {
        let readline = rl.readline("> ");
        match readline {
            Ok(line) => {
                handle_line(line, &mut env, &mut rl);
            }
            Err(ReadlineError::Interrupted) => {
                println!("Ctrl-C pressed. Exiting.");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Ctrl-D pressed. Exiting.");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    save_history(&mut rl);

    Ok(())
}

// TODO: This should be in lib in some way.
enum Fixme {
    Expr(Expr),
    Stmt(Stmt),
}

fn repl_parser<'src>() -> impl Parser<'src, &'src str, Fixme> {
    let stmt = Stmt::parser().map(|s| Fixme::Stmt(s));
    let expr = Expr::parser().map(|e| Fixme::Expr(e));
    stmt.or(expr)
}

fn handle_line<'cfg, 'src>(line: String, env: &mut Env<'cfg>, rl: &mut Editor<(), FileHistory>) {
    match repl_parser().parse(&line).into_result() {
        Ok(fixme) => {
            rl.add_history_entry(&line).unwrap();
            match fixme {
                Fixme::Expr(expr) => match expr.resolve(env) {
                    Ok(expr) => println!("{}", expr.normalize()),
                    Err(err) => println!("Error: {:?}", err),
                },
                Fixme::Stmt(stmts) => {
                    if let Err(err) = stmts.resolve(env) {
                        println!("Error: {:?}", err);
                    }
                }
            }
        }
        Err(_errors) => {
            println!("Error: Invalid syntax");
            // for error in errors {
            //     println!("{}", error)
            // }
        }
    }
}

const HISTDIR: &'static str = "grapl";
const HISTFILE: &'static str = "grapl.history";

fn load_history(rl: &mut Editor<(), FileHistory>) {
    with_histfile(rl, |rl, path| {
        rl.load_history(path).ok();
    });
}

fn save_history(rl: &mut Editor<(), FileHistory>) {
    with_histfile(rl, |rl, path| {
        rl.save_history(path).ok();
    });
}

fn with_histfile<F>(rl: &mut Editor<(), FileHistory>, func: F)
where
    F: Fn(&mut Editor<(), FileHistory>, &PathBuf),
{
    if let Ok(mut path) = get_xdg_state_dir() {
        path.push(HISTDIR);
        if fs::create_dir_all(&path).is_ok() {
            path.push(HISTFILE);
            func(rl, &path)
        }
    }
}

fn get_xdg_state_dir() -> Result<PathBuf, XdgError> {
    let xdg = Xdg::new()?;
    let state_dir = xdg.state()?;
    Ok(state_dir)
}
