extern crate clap;
extern crate fibers;
extern crate hb;
extern crate serdeconv;
extern crate slog;
extern crate sloggers;
#[macro_use]
extern crate trackable;

use std::fs::File;
use std::io;
use clap::{App, Arg, SubCommand};
use fibers::{Executor, InPlaceExecutor, ThreadPoolExecutor, Spawn};
use hb::Error;
use slog::Logger;
use sloggers::Build;
use trackable::error::Failure;

fn main() {
    let matches = App::new("hb")
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("LOGLEVEL")
                .short("l")
                .long("loglevel")
                .takes_value(true)
                .default_value("warning"),
        )
        .subcommand(
            SubCommand::with_name("run")
                .arg(
                    Arg::with_name("INPUT")
                        .short("i")
                        .long("input")
                        .takes_value(true)
                        .default_value("-"),
                )
                .arg(
                    Arg::with_name("OUTPUT")
                        .short("o")
                        .long("output")
                        .takes_value(true)
                        .default_value("-"),
                )
                .arg(
                    Arg::with_name("CONCURRENCY")
                        .short("c")
                        .long("concurrency")
                        .takes_value(true)
                        .default_value("32"),
                )
                .arg(
                    Arg::with_name("THREADS")
                        .short("t")
                        .long("threads")
                        .takes_value(true)
                        .default_value("1"),
                ),
        )
        .get_matches();

    let loglevel: sloggers::types::Severity =
        track_try_unwrap!(matches.value_of("LOGLEVEL").unwrap().parse());
    let logger = track_try_unwrap!(
        sloggers::terminal::TerminalLoggerBuilder::new()
            .level(loglevel)
            .destination(sloggers::terminal::Destination::Stderr)
            .build()
    );

    if let Some(matches) = matches.subcommand_matches("run") {
        let requests = match matches.value_of("INPUT").unwrap() {
            "-" => track_try_unwrap!(hb::run::RequestQueue::read_from(io::stdin())),
            filepath => {
                let f = track_try_unwrap!(File::open(filepath).map_err(Error::from));
                track_try_unwrap!(hb::run::RequestQueue::read_from(f))
            }
        };

        let threads: usize =
            track_try_unwrap!(matches.value_of("THREADS").unwrap().parse().map_err(
                Failure::from_error,
            ));

        let concurrency =
            track_try_unwrap!(matches.value_of("CONCURRENCY").unwrap().parse().map_err(
                Failure::from_error,
            ));
        let responses = if threads == 1 {
            let executor = track_try_unwrap!(InPlaceExecutor::new().map_err(Error::from));
            track_try_unwrap!(execute_runner(logger, executor, concurrency, requests))
        } else {
            let executor = track_try_unwrap!(
                ThreadPoolExecutor::with_thread_count(threads).map_err(Error::from)
            );
            track_try_unwrap!(execute_runner(logger, executor, concurrency, requests))
        };

        match matches.value_of("OUTPUT").unwrap() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, io::stdout()));
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, f));
            }
        }
    } else {
        println!("Usage: {}", matches.usage());
        std::process::exit(1);
    }
}

fn execute_runner<E: Executor>(
    logger: Logger,
    mut executor: E,
    concurrency: usize,
    requests: hb::run::RequestQueue,
) -> hb::Result<Vec<hb::run::RequestResult>> {
    let runner = hb::run::RunnerBuilder::new()
        .concurrency(concurrency)
        .finish(logger, executor.handle(), requests);
    let monitor = executor.handle().spawn_monitor(runner);
    let result = track!(executor.run_fiber(monitor).map_err(Error::from))?;
    track!(result.map_err(Error::from))
}
