//! Offline lexical retrieval only. No embeddings, model downloads or network access.
use crate::error::{AppError, AppResult};
use rusqlite::{params_from_iter, Connection};
use std::collections::{BTreeMap, BTreeSet};
const LIMIT: usize = 500;
fn error(_: impl std::fmt::Debug) -> AppError {
    AppError::new(
        "search_storage",
        "本地搜索索引不可用，请检查数据目录并重启应用。",
    )
}

pub fn init(db: &Connection) -> AppResult<()> {
    let existed: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='terms_fts')",
            [],
            |r| r.get(0),
        )
        .map_err(error)?;
    let tx = db.unchecked_transaction().map_err(error)?;
    tx.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS terms_fts USING fts5(original,translation,domain,sense,content='terms',content_rowid='id',tokenize='trigram');
      CREATE TRIGGER IF NOT EXISTS terms_search_insert AFTER INSERT ON terms BEGIN
        INSERT INTO terms_fts(rowid,original,translation,domain,sense) VALUES(new.id,new.original,new.translation,new.domain,new.sense); END;
      CREATE TRIGGER IF NOT EXISTS terms_search_delete AFTER DELETE ON terms BEGIN
        INSERT INTO terms_fts(terms_fts,rowid,original,translation,domain,sense) VALUES('delete',old.id,old.original,old.translation,old.domain,old.sense); END;
      CREATE TRIGGER IF NOT EXISTS terms_search_update AFTER UPDATE ON terms BEGIN
        INSERT INTO terms_fts(terms_fts,rowid,original,translation,domain,sense) VALUES('delete',old.id,old.original,old.translation,old.domain,old.sense);
        INSERT INTO terms_fts(rowid,original,translation,domain,sense) VALUES(new.id,new.original,new.translation,new.domain,new.sense); END;
      INSERT OR IGNORE INTO metadata(key,value) VALUES('search_index_version',0);
      INSERT INTO terms_fts(terms_fts,rank) VALUES('secure-delete',1);") .map_err(error)?;
    let version: i64 = tx
        .query_row(
            "SELECT value FROM metadata WHERE key='search_index_version'",
            [],
            |r| r.get(0),
        )
        .map_err(error)?;
    if !existed || version == 0 {
        tx.execute("INSERT INTO terms_fts(terms_fts) VALUES('rebuild')", [])
            .map_err(error)?;
        tx.execute(
            "UPDATE metadata SET value=1 WHERE key='search_index_version'",
            [],
        )
        .map_err(error)?;
    }
    tx.commit().map_err(error)
}

fn literal(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
fn keyword_sql(tokens: &[&str], all: bool) -> (String, Vec<String>) {
    let mut args = vec![];
    let mut clauses = vec![];
    let mut ranks = vec![];
    for token in tokens {
        let n = args.len() + 1;
        let escaped = literal(token);
        args.extend([
            format!("%{escaped}%"),
            escaped.clone(),
            format!("{escaped}%"),
        ]);
        clauses.push(format!("(original LIKE ?{n} ESCAPE '\\' OR translation LIKE ?{n} ESCAPE '\\' OR domain LIKE ?{n} ESCAPE '\\' OR sense LIKE ?{n} ESCAPE '\\')"));
        let exact = n + 1;
        let prefix = n + 2;
        ranks.push(format!("CASE WHEN original LIKE ?{exact} ESCAPE '\\' THEN 1000 WHEN translation LIKE ?{exact} ESCAPE '\\' THEN 850 WHEN original LIKE ?{prefix} ESCAPE '\\' THEN 700 WHEN translation LIKE ?{prefix} ESCAPE '\\' THEN 600 WHEN original LIKE ?{n} ESCAPE '\\' THEN 500 WHEN translation LIKE ?{n} ESCAPE '\\' THEN 450 WHEN domain LIKE ?{n} ESCAPE '\\' THEN 250 ELSE 200 END"));
    }
    let join = if all { " AND " } else { " OR " };
    (
        format!(
            "SELECT id,({}) AS relevance FROM terms WHERE {} ORDER BY relevance DESC,id DESC",
            ranks.join(" + "),
            clauses.join(join)
        ),
        args,
    )
}

#[derive(Clone)]
struct Candidate {
    id: i64,
    fields: [String; 4],
}
fn candidates(db: &Connection, sql: &str, args: &[String]) -> AppResult<Vec<Candidate>> {
    let mut stmt = db.prepare(sql).map_err(error)?;
    let rows = stmt
        .query_map(params_from_iter(args), |r| {
            Ok(Candidate {
                id: r.get(0)?,
                fields: [
                    r.get::<_, String>(1)?.to_lowercase(),
                    r.get::<_, String>(2)?.to_lowercase(),
                    r.get::<_, String>(3)?.to_lowercase(),
                    r.get::<_, String>(4)?.to_lowercase(),
                ],
            })
        })
        .map_err(error)?;
    rows.collect::<Result<_, _>>().map_err(error)
}
fn near(a: &str, b: &str) -> bool {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    if !(3..=64).contains(&a.len()) || b.len() > 64 {
        return false;
    }
    let limit = if a.len() <= 4 { 1 } else { 2 };
    if a.len().abs_diff(b.len()) > limit {
        return false;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, left) in a.iter().enumerate() {
        let mut next = vec![i + 1; b.len() + 1];
        for (j, right) in b.iter().enumerate() {
            next[j + 1] = (next[j] + 1)
                .min(row[j + 1] + 1)
                .min(row[j] + usize::from(left != right));
        }
        if *next.iter().min().unwrap() > limit {
            return false;
        }
        row = next;
    }
    row[b.len()] <= limit
}
fn rank(candidate: &Candidate, tokens: &[&str]) -> i64 {
    tokens
        .iter()
        .map(|token| {
            let token = token.to_lowercase();
            let mut best = 0;
            for (i, field) in candidate.fields.iter().enumerate() {
                let score = if field == &token {
                    [1000, 850, 600, 300][i]
                } else if field.starts_with(&token) {
                    [700, 600, 350, 250][i]
                } else if field.contains(&token) {
                    [500, 450, 250, 200][i]
                } else if i < 2
                    && field
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|word| near(&token, word))
                {
                    150
                } else {
                    0
                };
                best = best.max(score);
            }
            best
        })
        .sum()
}

pub fn page(db: &Connection, query: &str, mode: &str, offset: u32) -> AppResult<(Vec<i64>, i64)> {
    if !["keyword", "fuzzy"].contains(&mode) {
        return Err(AppError::new(
            "search_mode",
            "搜索模式无效，请选择关键词或模糊相关。",
        ));
    }
    if query.chars().count() > 200 {
        return Err(AppError::new("search_query", "搜索内容最多 200 字符。"));
    }
    let query = query.trim();
    if query.is_empty() {
        let total = db
            .query_row("SELECT COUNT(*) FROM terms", [], |r| r.get(0))
            .map_err(error)?;
        let ids = db
            .prepare("SELECT id FROM terms ORDER BY id DESC LIMIT 30 OFFSET ?")
            .map_err(error)?
            .query_map([offset], |r| r.get(0))
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)?;
        return Ok((ids, total));
    }
    let tokens: Vec<_> = query.split_whitespace().collect();
    if tokens.len() > 8 {
        return Err(AppError::new("search_query", "搜索最多支持 8 个关键词。"));
    }
    if mode == "keyword" {
        let (sql, args) = keyword_sql(&tokens, true);
        let total = db
            .query_row(
                &format!("SELECT COUNT(*) FROM ({sql})"),
                params_from_iter(&args),
                |r| r.get(0),
            )
            .map_err(error)?;
        let ids = db
            .prepare(&format!("{sql} LIMIT 30 OFFSET {offset}"))
            .map_err(error)?
            .query_map(params_from_iter(&args), |r| r.get(0))
            .map_err(error)?
            .collect::<Result<_, _>>()
            .map_err(error)?;
        return Ok((ids, total));
    }
    let mut pool = BTreeMap::new();
    let recent = candidates(
        db,
        &format!(
            "SELECT id,original,translation,domain,sense FROM terms ORDER BY id DESC LIMIT {LIMIT}"
        ),
        &[],
    )?;
    for item in recent {
        pool.insert(item.id, item);
    }
    let (sql, args) = keyword_sql(&tokens, false);
    let matched=candidates(db,&format!("SELECT id,original,translation,domain,sense FROM terms WHERE id IN (SELECT id FROM ({sql}) LIMIT {LIMIT})"),&args)?;
    for item in matched {
        pool.insert(item.id, item);
    }
    let grams: BTreeSet<_> = tokens
        .iter()
        .flat_map(|t| {
            let chars: Vec<_> = t.chars().collect();
            chars
                .windows(3)
                .take(8)
                .map(|w| w.iter().collect::<String>())
                .collect::<Vec<_>>()
        })
        .take(8)
        .collect();
    if !grams.is_empty() {
        let expression = grams
            .into_iter()
            .map(|g| format!("\"{}\"", g.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let indexed=candidates(db,&format!("SELECT id,original,translation,domain,sense FROM terms WHERE id IN (SELECT rowid FROM terms_fts WHERE terms_fts MATCH ? ORDER BY bm25(terms_fts) LIMIT {LIMIT})"),&[expression])?;
        for item in indexed {
            pool.insert(item.id, item);
        }
    }
    let mut ranked: Vec<_> = pool
        .values()
        .map(|item| (rank(item, &tokens), item.id))
        .filter(|(score, _)| *score > 0)
        .collect();
    ranked.sort_unstable_by(|a, b| b.cmp(a));
    let total = ranked.len() as i64;
    Ok((
        ranked
            .into_iter()
            .skip(offset as usize)
            .take(30)
            .map(|(_, id)| id)
            .collect(),
        total,
    ))
}

#[cfg(test)]
mod tests;
