#[macro_use]
extern crate trackable;

use clap::Parser;
use fibers::{Executor, InPlaceExecutor, Spawn, ThreadPoolExecutor};
use hb::Error;
use std::fs::File;
use std::io::{self, BufReader};

#[derive(Parser)]
#[clap(version)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Run(RunCommand),
    Get(GetCommand),
    Head(HeadCommand),
    Delete(DeleteCommand),
    Put(PutCommand),
    Post(PostCommand),
    Summary(SummaryCommand),
    TimeSeries(TimeSeriesCommand),
}

fn main() {
    env_logger::init();

    let args = Args::parse();
    match args.command {
        Command::Run(c) => c.execute(),
        Command::Get(c) => c.execute(),
        Command::Head(c) => c.execute(),
        Command::Delete(c) => c.execute(),
        Command::Put(c) => c.execute(),
        Command::Post(c) => c.execute(),
        Command::Summary(c) => c.execute(),
        Command::TimeSeries(c) => c.execute(),
    }
}

fn execute_runner<E: Executor>(
    mut executor: E,
    concurrency: usize,
    connection_pool_size: usize,
    requests: &hb::run::RequestQueue,
) -> hb::Result<Vec<hb::run::RequestResult>> {
    let runner = hb::run::RunnerBuilder::new()
        .concurrency(concurrency)
        .connection_pool_size(connection_pool_size)
        .finish(&executor.handle(), requests);
    let monitor = executor.handle().spawn_monitor(runner);
    let result = track!(executor.run_fiber(monitor).map_err(Error::from))?;
    track!(result.map_err(Error::from))
}

#[derive(clap::Args)]
struct RunCommand {
    #[clap(short, long, default_value = "-")]
    input: String,

    #[clap(short, long, default_value = "-")]
    output: String,

    #[clap(short, long, default_value_t = 32)]
    concurrency: usize,

    #[clap(long, default_value_t = 4096)]
    connection_pool_size: usize,

    #[clap(short, long, default_value_t = 2)]
    threads: usize,
}

impl RunCommand {
    fn execute(&self) {
        let requests = match self.input.as_str() {
            "-" => {
                let stdin = io::stdin();
                track_try_unwrap!(hb::run::RequestQueue::read_from(stdin.lock()))
            }
            filepath => {
                let f = track_try_unwrap!(File::open(filepath).map_err(Error::from));
                track_try_unwrap!(hb::run::RequestQueue::read_from(BufReader::new(f)))
            }
        };

        let responses = if self.threads == 1 {
            let executor = track_try_unwrap!(InPlaceExecutor::new().map_err(Error::from));
            track_try_unwrap!(execute_runner(
                executor,
                self.concurrency,
                self.connection_pool_size,
                &requests
            ))
        } else {
            let executor = track_try_unwrap!(
                ThreadPoolExecutor::with_thread_count(self.threads).map_err(Error::from)
            );
            track_try_unwrap!(execute_runner(
                executor,
                self.concurrency,
                self.connection_pool_size,
                &requests
            ))
        };

        match self.output.as_str() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, io::stdout()));
                println!();
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, f));
            }
        }
    }
}

#[derive(clap::Args)]
struct RequestCommand {
    urls: Vec<url::Url>,

    #[clap(short = 'n', long, default_value_t = 10)]
    requests: usize,

    #[clap(short, long, default_value = "-")]
    output: String,

    #[clap(short, long, default_value_t = 32)]
    concurrency: usize,

    #[clap(long, default_value_t = 4096)]
    connection_pool_size: usize,

    #[clap(short, long, default_value_t = 2)]
    threads: usize,
}

impl RequestCommand {
    fn execute(&self, method: hb::request::Method, content: Option<&hb::request::Content>) {
        let requests = self
            .urls
            .iter()
            .cycle()
            .zip(0..self.requests)
            .map(|(url, _)| hb::request::Request {
                method,
                url: url.clone(),
                content: content.cloned(),
                timeout: None,
                start_time: None,
            })
            .collect();
        let requests = hb::run::RequestQueue::new(requests);
        let responses = if self.threads == 1 {
            let executor = track_try_unwrap!(InPlaceExecutor::new().map_err(Error::from));
            track_try_unwrap!(execute_runner(
                executor,
                self.concurrency,
                self.connection_pool_size,
                &requests
            ))
        } else {
            let executor = track_try_unwrap!(
                ThreadPoolExecutor::with_thread_count(self.threads).map_err(Error::from)
            );
            track_try_unwrap!(execute_runner(
                executor,
                self.concurrency,
                self.connection_pool_size,
                &requests
            ))
        };
        match self.output.as_str() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, io::stdout()));
                println!();
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&responses, f));
            }
        }
    }
}

#[derive(clap::Args)]
struct GetCommand {
    #[clap(flatten)]
    request: RequestCommand,
}

impl GetCommand {
    fn execute(&self) {
        self.request.execute(hb::request::Method::Get, None);
    }
}

#[derive(clap::Args)]
struct HeadCommand {
    #[clap(flatten)]
    request: RequestCommand,
}

impl HeadCommand {
    fn execute(&self) {
        self.request.execute(hb::request::Method::Head, None);
    }
}

#[derive(clap::Args)]
struct DeleteCommand {
    #[clap(flatten)]
    request: RequestCommand,
}

impl DeleteCommand {
    fn execute(&self) {
        self.request.execute(hb::request::Method::Delete, None);
    }
}

#[derive(clap::Args)]
struct PutCommand {
    #[clap(flatten)]
    request: RequestCommand,
}

impl PutCommand {
    fn execute(&self) {
        self.request.execute(hb::request::Method::Put, None);
    }
}

#[derive(clap::Args)]
struct PostCommand {
    #[clap(flatten)]
    request: RequestCommand,

    #[clap(long)]
    content_length: Option<usize>,

    #[clap(long)]
    content: Option<String>,
}

impl PostCommand {
    fn execute(&self) {
        let content = if let Some(text) = &self.content {
            Some(hb::request::Content::Text(text.to_owned()))
        } else if let Some(len) = self.content_length {
            Some(hb::request::Content::Size(len))
        } else {
            None
        };
        self.request
            .execute(hb::request::Method::Post, content.as_ref())
    }
}

#[derive(clap::Args)]
struct SummaryCommand {
    #[clap(short, long, default_value = "-")]
    input: String,

    #[clap(short, long, default_value = "-")]
    output: String,
}

impl SummaryCommand {
    fn execute(&self) {
        let responses = match self.input.as_str() {
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
        match self.output.as_str() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, io::stdout()));
                println!();
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, f));
            }
        }
    }
}

#[derive(clap::Args)]
struct TimeSeriesCommand {
    #[clap(short, long, default_value = "-")]
    input: String,

    #[clap(short, long, default_value = "-")]
    output: String,
}

impl TimeSeriesCommand {
    fn execute(&self) {
        let responses = match self.input.as_str() {
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
        match self.output.as_str() {
            "-" => {
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, io::stdout()));
                println!();
            }
            filepath => {
                let f = track_try_unwrap!(File::create(filepath).map_err(Error::from));
                track_try_unwrap!(serdeconv::to_json_writer_pretty(&summary, f));
            }
        }
    }
}
