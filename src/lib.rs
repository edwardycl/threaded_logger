use std::{borrow::Cow, collections::VecDeque, error, fmt, sync::Mutex};

use log::{LevelFilter, Log, Metadata, Record};
use once_cell::sync::{Lazy, OnceCell};
use tokio::task::JoinHandle;

static INNER_LOGGER: OnceCell<Box<dyn Log>> = OnceCell::new();
static HANDLES: Lazy<Mutex<VecDeque<JoinHandle<()>>>> = Lazy::new(|| Mutex::new(VecDeque::new()));

struct ThreadedLogger {
    logger: &'static dyn Log,
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
        let handle = tokio::spawn(async move {
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
        });

        HANDLES.lock().unwrap().push_back(handle);
    }

    fn flush(&self) {
        self.logger.flush()
    }
}

pub fn try_init(
    logger: impl Log + 'static,
    max_level: LevelFilter,
) -> Result<(), ThreadedLoggerError> {
    INNER_LOGGER
        .set(Box::new(logger))
        .map_err(|_| ThreadedLoggerError(()))?;
    let threaded_logger = ThreadedLogger {
        logger: unsafe { INNER_LOGGER.get_unchecked() },
    };

    let r = log::set_boxed_logger(Box::new(threaded_logger)).map_err(|_| ThreadedLoggerError(()));
    if r.is_ok() {
        log::set_max_level(max_level);
    }

    tokio::spawn(async move {
        loop {
            tokio::task::yield_now().await;
            let handle = HANDLES.lock().unwrap().pop_front();
            if let Some(handle) = handle {
                handle.await.ok();
            }
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
