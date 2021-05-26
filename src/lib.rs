use std::{borrow::Cow, error, fmt};

use crossbeam::channel::{unbounded, Sender};
use log::{LevelFilter, Log, Metadata, Record};
use once_cell::sync::OnceCell;

static INNER_LOGGER: OnceCell<Box<dyn Log>> = OnceCell::new();

struct ThreadedLogger {
    logger: &'static dyn Log,
    sender: Sender<Box<dyn FnOnce() + Send>>,
}

impl Log for ThreadedLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.logger.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        let level = record.metadata().level();
        let target = record.metadata().target().to_owned();

        let args_str = if let Some(s) = record.args().as_str() {
            Cow::Borrowed(s)
        } else {
            Cow::Owned(fmt::format(*record.args()))
        };

        let module_path = if let Some(s) = record.module_path_static() {
            Some(Cow::Borrowed(s))
        } else {
            record.module_path().map(|s| Cow::Owned(s.to_owned()))
        };

        let file = if let Some(s) = record.file_static() {
            Some(Cow::Borrowed(s))
        } else {
            record.file().map(|s| Cow::Owned(s.to_owned()))
        };

        let line = record.line();

        let logger_ = self.logger.clone();
        let log = move || {
            let metadata = Metadata::builder().level(level).target(&target).build();

            logger_.log(
                &Record::builder()
                    .metadata(metadata)
                    .args(format_args!("{}", args_str))
                    .module_path(module_path.as_deref())
                    .file(file.as_deref())
                    .line(line)
                    .build(),
            );
        };

        self.sender.send(Box::new(log)).ok();
    }

    fn flush(&self) {
        self.logger.flush()
    }
}

pub fn try_init(
    logger: impl Log + 'static,
    max_level: LevelFilter,
) -> Result<(), ThreadedLoggerError> {
    let (sender, receiver) = unbounded();

    INNER_LOGGER
        .set(Box::new(logger))
        .map_err(|_| ThreadedLoggerError(()))?;
    let threaded_logger = ThreadedLogger {
        logger: unsafe { INNER_LOGGER.get_unchecked() },
        sender,
    };

    let r = log::set_boxed_logger(Box::new(threaded_logger)).map_err(|_| ThreadedLoggerError(()));
    if r.is_ok() {
        log::set_max_level(max_level);
    }

    tokio::task::spawn_blocking(move || loop {
        if let Ok(log) = receiver.recv() {
            log();
        }
    });

    r
}

pub fn init(logger: impl Log + 'static, max_level: LevelFilter) {
    try_init(logger, max_level).unwrap();
}

#[derive(Debug)]
pub struct ThreadedLoggerError(());

impl fmt::Display for ThreadedLoggerError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str("attempted to set a logger more than once")
    }
}

impl error::Error for ThreadedLoggerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn threaded_env_logger() {
        let logger = env_logger::builder().build();
        let filter = logger.filter();

        init(logger, filter);

        let now = std::time::Instant::now();
        for i in 0..100000 {
            log::info!("{}", i);
        }
        let t = now.elapsed().as_micros();
        println!("time elapsed: {}µs", t);
    }
}
