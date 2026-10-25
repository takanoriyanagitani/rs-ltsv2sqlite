use std::io;
use std::path::Path;

use io::BufRead;

use serde::Serialize;
use serde::Serializer;
use serde::ser::SerializeMap;

use rusqlite::Connection;
use rusqlite::Statement;
use rusqlite::Transaction;

#[derive(Debug, Clone, Copy)]
pub struct Row<'a> {
    pub timestamp: &'a str,
    pub severity: &'a str,
    pub message: &'a str,
    pub others_json: &'a [u8],
}

pub struct Keys {
    pub timestamp: String,
    pub severity: String,
    pub message: String,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            timestamp: "timestamp".into(),
            severity: "severity".into(),
            message: "message".into(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Separator {
    pub pair: char,
    pub kv: char,
}

impl Default for Separator {
    fn default() -> Self {
        Self {
            pair: '\t',
            kv: ':',
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StrRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct StrPairRange {
    pub key: StrRange,
    pub val: StrRange,
}

pub struct PairsView<'a> {
    pub source: &'a str,
    pub indices: &'a [StrPairRange],
}

impl<'a> Serialize for PairsView<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.indices.len()))?;
        for spr in self.indices {
            let key: &StrRange = &spr.key;
            let val: &StrRange = &spr.val;

            let ks: usize = key.start;
            let ke: usize = key.end;

            let vs: usize = val.start;
            let ve: usize = val.end;

            let k: &str = &self.source[ks..ke];
            let v: &str = &self.source[vs..ve];

            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

pub struct LtsvRow<'a> {
    pub timestamp: &'a str,
    pub severity: &'a str,
    pub message: &'a str,
    pub pairs: PairsView<'a>,
}

impl<'a> LtsvRow<'a> {
    pub fn pairs2json(&self, jsbuf: &mut Vec<u8>) -> Result<(), io::Error> {
        jsbuf.clear();
        serde_json::to_writer(jsbuf, &self.pairs)?;
        Ok(())
    }
}

#[repr(u8)]
#[derive(Default, Debug, Clone, Copy)]
pub enum Severity {
    #[default]
    Unspecified = 0,
    Trace = 1,
    Debug = 5,
    Info = 9,
    Warn = 13,
    Error = 17,
    Fatal = 21,
}

pub fn sevstr2u8<S>(conv: S) -> impl Fn(&str) -> Result<u8, io::Error>
where
    S: Fn(&str) -> Result<Severity, io::Error>,
{
    move |s: &str| conv(s).map(|sev| sev as u8)
}

pub fn sevstr2enum_simple(s: &str) -> Result<Severity, io::Error> {
    match s {
        "trace" | "TRACE" => Ok(Severity::Trace),
        "debug" | "DEBUG" => Ok(Severity::Debug),
        "info" | "INFO" => Ok(Severity::Info),
        "warn" | "WARN" | "warning" | "WARNING" => Ok(Severity::Warn),
        "error" | "ERROR" => Ok(Severity::Error),
        "fatal" | "FATAL" | "critical" | "CRITICAL" => Ok(Severity::Fatal),
        _ => Ok(Severity::default()),
    }
}

pub fn timestamp2unixtime_us_time(s: &str) -> Result<i64, io::Error> {
    let odt: time::OffsetDateTime =
        time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map_err(io::Error::other)?;
    let ns: i128 = odt.unix_timestamp_nanos();
    let us: i128 = ns / 1000;
    us.try_into().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unable to convert to micro seconds: {e}"),
        )
    })
}

pub struct Converter<T, S> {
    pub timestamp2unixtime_us: T,
    pub severity2enum: S,
}

impl<T, S> Converter<T, S> {
    pub fn into_row_saver(self, mut stmt: Statement) -> impl FnMut(&Row) -> Result<usize, io::Error>
    where
        T: Fn(&str) -> Result<i64, io::Error>,
        S: Fn(&str) -> Result<u8, io::Error>,
    {
        move |row: &Row| {
            let ms: &mut Statement = &mut stmt;
            let timestamp: i64 = (self.timestamp2unixtime_us)(row.timestamp)?;
            let severity: u8 = (self.severity2enum)(row.severity)?;
            ms.execute((timestamp, severity, row.message, row.others_json))
                .map_err(io::Error::other)
        }
    }
}

pub fn stmt2row_saver_default(stmt: Statement) -> impl FnMut(&Row) -> Result<usize, io::Error> {
    Converter {
        timestamp2unixtime_us: timestamp2unixtime_us_time,
        severity2enum: sevstr2u8(sevstr2enum_simple),
    }
    .into_row_saver(stmt)
}

/// Creates the view from the ltsv line.
///
/// Example
///
/// ```markdown
/// 0123456789ABCDEFGHIJKLMNOPQRSTUV
/// date:10-21 body:helo
/// ^   ^^    ^^
/// |   ||    ||__ 10+1=11
/// |   ||    |__ 0+10=10
/// |   ||__ 4+1=5
/// |   |__ 0+4=4
/// |__ 0
/// ```
pub fn ltsv2view<'a>(
    ltsv: &'a str,
    buf: &'a mut Vec<StrPairRange>,
    sep: Separator,
    keys: &Keys,
) -> LtsvRow<'a> {
    buf.clear();

    let mut cursor: usize = 0;
    let pairs = ltsv.split(sep.pair);

    let mut timestamp = "";
    let mut severity = "";
    let mut message = "";

    for pair in pairs {
        let okv: Option<(&str, &str)> = pair.split_once(sep.kv);
        if let Some(kv) = okv {
            let (key, val) = kv;

            let k_start: usize = cursor;
            let k_end: usize = k_start + key.len();

            let v_start: usize = k_end + 1;
            let v_end: usize = v_start + val.len();

            let mut others = true;

            if key == keys.timestamp {
                others = false;
                timestamp = val;
            }

            if key == keys.severity {
                others = false;
                severity = val;
            }

            if key == keys.message {
                others = false;
                message = val;
            }

            if others {
                buf.push(StrPairRange {
                    key: StrRange {
                        start: k_start,
                        end: k_end,
                    },
                    val: StrRange {
                        start: v_start,
                        end: v_end,
                    },
                });
            }
        }

        cursor += pair.len() + 1;
    }
    let pairs = PairsView {
        source: ltsv,
        indices: buf,
    };
    LtsvRow {
        timestamp,
        severity,
        message,
        pairs,
    }
}

pub fn lines2sqlite<R, S>(
    mut lines: R,
    mut saver: S,
    sep: Separator,
    keys: &Keys,
) -> Result<usize, io::Error>
where
    R: BufRead,
    S: FnMut(&Row) -> Result<usize, io::Error>,
{
    let mut buf4line: String = String::default();
    let mut tot: usize = 0;
    let mut buf: Vec<StrPairRange> = vec![];
    let mut buf4json: Vec<u8> = vec![];

    loop {
        buf4line.clear();
        let sz: usize = lines.read_line(&mut buf4line)?;
        if 0 == sz {
            return Ok(tot);
        }

        let line: &str = &buf4line;
        let nolf: &str = line.trim_end();
        let lrow: LtsvRow = ltsv2view(nolf, &mut buf, sep, keys);
        buf4json.clear();
        lrow.pairs2json(&mut buf4json)?;
        let row = Row {
            timestamp: lrow.timestamp,
            severity: lrow.severity,
            message: lrow.message,
            others_json: &buf4json,
        };
        let chg: usize = saver(&row)?;
        tot += chg;
    }
}

pub const INSERT_SQL: &str = r#"
    INSERT INTO ltsv(timestamp, severity, message, others)
    VALUES(?1, ?2, ?3, ?4)
"#;

pub const CREATE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS ltsv (
        timestamp INTEGER NOT NULL,
        severity INTEGER NOT NULL,
        message TEXT NOT NULL,
        others BLOB
    )
"#;

pub fn stdin2lines2sqlite2fs_default_converter<P>(
    path2sqlite: P,
    sep: Separator,
    keys: &Keys,
    create_sql: &str,
    insert_sql: &str,
) -> Result<usize, io::Error>
where
    P: AsRef<Path>,
{
    if path2sqlite.as_ref().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty path for sqlite got",
        ));
    }
    let mut conn = Connection::open(path2sqlite).map_err(io::Error::other)?;
    conn.execute(create_sql, ()).map_err(io::Error::other)?;

    let tx: Transaction = conn.transaction().map_err(io::Error::other)?;
    let stmt: Statement = tx.prepare(insert_sql).map_err(io::Error::other)?;

    let saver = stmt2row_saver_default(stmt);
    let chg: usize = lines2sqlite(io::stdin().lock(), saver, sep, keys)?;

    tx.commit().map_err(io::Error::other)?;
    conn.close().map_err(|p| p.1).map_err(io::Error::other)?;

    Ok(chg)
}

pub fn stdin2lines2sqlite2fs_default<P>(path2sqlite: P) -> Result<usize, io::Error>
where
    P: AsRef<Path>,
{
    stdin2lines2sqlite2fs_default_converter(
        path2sqlite,
        Separator::default(),
        &Keys::default(),
        CREATE_SQL,
        INSERT_SQL,
    )
}
