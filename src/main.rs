extern crate clap;
extern crate fibers;
extern crate hb;
extern crate serdeconv;
extern crate slog;
extern crate sloggers;
#[macro_use]
extern crate trackable;
extern crate url;

use clap::{App, Arg, ArgMatches, SubCommand};
use fibers::{Executor, InPlaceExecutor, Spawn, ThreadPoolExecutor};
use hb::Error;
use slog::Logger;
use sloggers::Build;
use std::fs::File;
use std::io::{self, BufReader};
use trackable::error::Failure;
use url::Url;

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
        .subcommand(SubCommandRun::app())
        .subcommand(SubCommandGet::app())
        .subcommand(SubCommandHead::app())
        .subcommand(SubCommandDelete::app())
        .subcommand(SubCommandPut::app())
        .subcommand(SubCommandPost::app())
        .subcommand(SubCommandSummary::app())
        .subcommand(SubCommandTimeSeries::app())
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
        SubCommandRun::execute(logger, matches);
    } else if let Some(matches) = matches.subcommand_matches("get") {
        SubCommandGet::execute(logger, matches);
    } else if let Some(matches) = matches.subcommand_matches("head") {
        SubCommandHead::execute(logger, matches);
    } else if let Some(matches) = matches.subcommand_matches("delete") {
        SubCommandDelete::execute(logger, matches);
    } else if let Some(matches) = matches.subcommand_matches("put") {
        SubCommandPut::execute(logger, matches);
    } else if let Some(matches) = matches.subcommand_matches("post") {
        SubCommandPost::execute(logger, matches);
    } else if let Some(matches) = matches.subcommand_matches("summary") {
        SubCommandSummary::execute(matches);
    } else if let Some(matches) = matches.subcommand_matches("time-series") {
        SubCommandTimeSeries::execute(matches);
    } else {
        println!("Usage: {}", matches.usage());
        std::process::exit(1);
    }
}

fn execute_runner<E: Executor>(
    logger: Logger,
    mut executor: E,
    concurrency: usize,
    requests: &hb::run::RequestQueue,
) -> hb::Result<Vec<hb::run::RequestResult>> {
    let runner = hb::run::RunnerBuilder::new()
        .concurrency(concurrency)
        .finish(logger, &executor.handle(), requests);
    let monitor = executor.handle().spawn_monitor(runner);
    let result = track!(executor.run_fiber(monitor).map_err(Error::from))?;
    track!(result.map_err(Error::from))
}

struct SubCommandRun;
impl SubCommandRun {
    fn app() -> App<'static, 'static> {
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
                    .default_value("2"),
            )
    }
    fn execute(logger: Logger, matches: &ArgMatches) {
        let requests = match matches.value_of("INPUT").unwrap() {
            "-" => {
                let stdin = io::stdin();
                track_try_unwrap!(hb::run::RequestQueue::read_from(stdin.lock()))
            }
            filepath => {
                let f = track_try_unwrap!(File::open(filepath).map_err(Error::from));
                track_try_unwrap!(hb::run::RequestQueue::read_from(BufReader::new(f)))
            }
        };

        let threads: usize = track_try_unwrap!(
            matches
                .value_of("THREADS")
                .unwrap()
                .parse()
                .map_err(Failure::from_error)
        );

        let concurrency = track_try_unwrap!(
            matches
                .value_of("CONCURRENCY")
                .unwrap()
                .parse()
                .map_err(Failure::from_error)
        );
        let responses = if threads == 1 {
            let executor = track_try_unwrap!(InPlaceExecutor::new().map_err(Error::from));
            track_try_unwrap!(execute_runner(logger, executor, concurrency, &requests))
        } else {
            let executor = track_try_unwrap!(
                ThreadPoolExecutor::with_thread_count(threads).map_err(Error::from)
            );
            track_try_unwrap!(execute_runner(logger, executor, concurrency, &requests))
        };

        match matches.value_of("OUTPUT").unwrap() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, io::stdout()));
                println!("");
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, f));
            }
        }
    }
}

struct SubCommandRequest;
impl SubCommandRequest {
    fn app(method: &'static str) -> App<'static, 'static> {
        SubCommand::with_name(method)
            .arg(
                Arg::with_name("URL")
                    .index(1)
                    .required(true)
                    .multiple(true)
                    .min_values(1),
            )
            .arg(
                Arg::with_name("REQUESTS")
                    .short("n")
                    .long("requests")
                    .takes_value(true)
                    .default_value("10"),
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
            )
    }
    fn execute(
        logger: Logger,
        matches: &ArgMatches,
        method: hb::request::Method,
        content: Option<hb::request::Content>,
    ) {
        let mut urls = Vec::new();
        for url in matches.values_of("URL").unwrap() {
            let url = track_try_unwrap!(Url::parse(url).map_err(Failure::from_error));
            urls.push(url);
        }
        let requests: usize = track_try_unwrap!(
            matches
                .value_of("REQUESTS",)
                .unwrap()
                .parse()
                .map_err(Failure::from_error,)
        );
        let threads: usize = track_try_unwrap!(
            matches
                .value_of("THREADS",)
                .unwrap()
                .parse()
                .map_err(Failure::from_error,)
        );
        let concurrency: usize = track_try_unwrap!(
            matches
                .value_of("CONCURRENCY",)
                .unwrap()
                .parse()
                .map_err(Failure::from_error,)
        );

        let requests = urls.iter()
            .cycle()
            .zip(0..requests)
            .map(|(url, _)| hb::request::Request {
                method,
                url: url.clone(),
                content: content.clone(),
                timeout: None,
                start_time: None,
            })
            .collect();
        let requests = hb::run::RequestQueue::new(requests);
        let responses = if threads == 1 {
            let executor = track_try_unwrap!(InPlaceExecutor::new().map_err(Error::from));
            track_try_unwrap!(execute_runner(logger, executor, concurrency, &requests))
        } else {
            let executor = track_try_unwrap!(
                ThreadPoolExecutor::with_thread_count(threads).map_err(Error::from)
            );
            track_try_unwrap!(execute_runner(logger, executor, concurrency, &requests))
        };
        match matches.value_of("OUTPUT").unwrap() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, io::stdout()));
                println!("");
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, f));
            }
        }
    }
}

struct SubCommandGet;
impl SubCommandGet {
    fn app() -> App<'static, 'static> {
        SubCommandRequest::app("get")
    }
    fn execute(logger: Logger, matches: &ArgMatches) {
        SubCommandRequest::execute(logger, matches, hb::request::Method::Get, None)
    }
}

struct SubCommandHead;
impl SubCommandHead {
    fn app() -> App<'static, 'static> {
        SubCommandRequest::app("head")
    }
    fn execute(logger: Logger, matches: &ArgMatches) {
        SubCommandRequest::execute(logger, matches, hb::request::Method::Head, None)
    }
}

struct SubCommandDelete;
impl SubCommandDelete {
    fn app() -> App<'static, 'static> {
        SubCommandRequest::app("delete")
    }
    fn execute(logger: Logger, matches: &ArgMatches) {
        SubCommandRequest::execute(logger, matches, hb::request::Method::Delete, None)
    }
}

struct SubCommandPut;
impl SubCommandPut {
    fn app() -> App<'static, 'static> {
        SubCommandRequest::app("put")
            .arg(
                Arg::with_name("CONTENT_LENGTH")
                    .long("content-length")
                    .takes_value(true),
            )
            .arg(Arg::with_name("CONTENT").long("content").takes_value(true))
    }
    fn execute(logger: Logger, matches: &ArgMatches) {
        let content = if let Some(text) = matches.value_of("CONTENT") {
            Some(hb::request::Content::Text(text.to_owned()))
        } else if let Some(len) = matches.value_of("CONTENT_LENGTH") {
            let len: usize = track_try_unwrap!(len.parse().map_err(Failure::from_error));
            Some(hb::request::Content::Size(len))
        } else {
            None
        };
        SubCommandRequest::execute(logger, matches, hb::request::Method::Put, content)
    }
}

struct SubCommandPost;
impl SubCommandPost {
    fn app() -> App<'static, 'static> {
        SubCommandRequest::app("post")
            .arg(
                Arg::with_name("CONTENT_LENGTH")
                    .long("content-length")
                    .takes_value(true),
            )
            .arg(Arg::with_name("CONTENT").long("content").takes_value(true))
    }
    fn execute(logger: Logger, matches: &ArgMatches) {
        let content = if let Some(text) = matches.value_of("CONTENT") {
            Some(hb::request::Content::Text(text.to_owned()))
        } else if let Some(len) = matches.value_of("CONTENT_LENGTH") {
            let len: usize = track_try_unwrap!(len.parse().map_err(Failure::from_error));
            Some(hb::request::Content::Size(len))
        } else {
            None
        };
        SubCommandRequest::execute(logger, matches, hb::request::Method::Post, content)
    }
}

struct SubCommandSummary;
impl SubCommandSummary {
    fn app() -> App<'static, 'static> {
        SubCommand::with_name("summary")
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
    }
    fn execute(matches: &ArgMatches) {
        let responses = match matches.value_of("INPUT").unwrap() {
            "-" => {
                let stdin = io::stdin();
                track_try_unwrap!(serdeconv::from_json_reader(stdin.lock()))
            }
            filepath => {
                let f = track_try_unwrap!(File::open(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::from_json_reader(BufReader::new(f)))
            }
        };
        let summary = hb::summary::Summary::new(responses);
        match matches.value_of("OUTPUT").unwrap() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, io::stdout()));
                println!("");
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, f));
            }
        }
    }
}

struct SubCommandTimeSeries;
impl SubCommandTimeSeries {
    fn app() -> App<'static, 'static> {
        SubCommand::with_name("time-series")
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
    }
    fn execute(matches: &ArgMatches) {
        let responses = match matches.value_of("INPUT").unwrap() {
            "-" => {
                let stdin = io::stdin();
                track_try_unwrap!(serdeconv::from_json_reader(stdin.lock()))
            }
            filepath => {
                let f = track_try_unwrap!(File::open(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::from_json_reader(BufReader::new(f)))
            }
        };
        let summary = hb::time_series::TimeSeries::new(responses);
        match matches.value_of("OUTPUT").unwrap() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, io::stdout()));
                println!("");
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, f));
            }
        }
    }
}
