use asa_sync_server::{
    store::{Limits, Store},
    web::{hash_password, router},
};
use std::{path::PathBuf, sync::Arc};
use zeroize::Zeroizing;
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("sync service: {e}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("serve");
    let path = PathBuf::from(
        std::env::var_os("SYNC_DATABASE").unwrap_or_else(|| "/data/sync.sqlite3".into()),
    );
    let mut limits = Limits::default();
    if let Ok(value) = std::env::var("SYNC_STORAGE_BYTES") {
        limits.storage = value.parse().map_err(|_| "invalid SYNC_STORAGE_BYTES")?;
        if limits.storage < 1 {
            return Err("invalid SYNC_STORAGE_BYTES".into());
        }
    }
    let err = |e: asa_sync_server::error::Error| e.body.code;
    if command == "restore" {
        let source = PathBuf::from(
            args.get(1)
                .ok_or("restore requires backup path; stop service first")?,
        );
        if source == path {
            return Err("backup and database paths must differ".into());
        }
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path.with_extension("lock"))
            .map_err(|_| "restore_lock")?;
        lock.try_lock()
            .map_err(|_| "stop service before restoring")?;
        let db = rusqlite::Connection::open_with_flags(
            &source,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| "backup_unreadable")?;
        let integrity: String = db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|_| "backup_invalid")?;
        if integrity != "ok" {
            return Err("backup_invalid".into());
        }
        let version: i64 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|_| "backup_schema")?;
        let valid:bool=db.query_row("SELECT COUNT(*)=4 FROM meta WHERE key IN ('workspace','history','generation','password')",[],|r|r.get(0)).map_err(|_|"backup_schema")?;
        if version != 1 || !valid {
            return Err("backup_schema".into());
        }
        let replacement = path.with_extension("restore.tmp");
        if replacement.exists() {
            return Err("restore temporary file exists; inspect it first".into());
        }
        db.backup("main", &replacement, None)
            .map_err(|_| "backup_copy")?;
        drop(db);
        {
            let restored = Store::open(&replacement, limits.clone()).map_err(err)?;
            restored.restored().map_err(err)?;
        }
        // Store close checkpoints SQLite before publication. Remove only superseded local journals.
        for suffix in ["-wal", "-shm"] {
            let journal = PathBuf::from(format!("{}{suffix}", path.display()));
            if journal.exists() {
                std::fs::remove_file(journal).map_err(|_| "old_journal_cleanup")?;
            }
        }
        std::fs::rename(replacement, &path).map_err(|_| "restore_publish")?;
        println!("Restored. Devices must re-enroll and confirm the new history.");
        return Ok(());
    }
    let store = Arc::new(Store::open(&path, limits).map_err(err)?);
    match command {
  "init"|"rotate-password"=>{
   let password=if let Some(p)=args.get(1){Zeroizing::new(std::fs::read_to_string(p).map_err(|_|"password_file_unreadable")?)}else{use std::io::Read;let mut value=String::new();std::io::stdin().take(8193).read_to_string(&mut value).map_err(|_|"password_stdin")?;Zeroizing::new(value)};
   let hash=hash_password(password.trim_end_matches(['\r','\n'])).map_err(err)?;store.set_password(&hash,command=="init").map_err(err)?;println!("Credentials configured; device tokens revoked.");
  },
  "backup"=>{let dest=PathBuf::from(args.get(1).ok_or("backup requires destination")?);if dest.exists(){return Err("backup destination exists".into());}store.backup(&dest).map_err(err)?;println!("Consistent backup completed.");},
  "status"=>{let ctx=store.context().map_err(err)?;println!("protocol={} revision={} generation={}",ctx.protocol,ctx.revision,ctx.memory_generation);},
  "serve"=>{
   store.password_hash().map_err(|_|"initialize dedicated sync password first")?;
   let lock=std::fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(path.with_extension("lock")).map_err(|_|"service_lock")?;lock.try_lock().map_err(|_|"service already running or restore in progress")?;
   let bind=std::env::var("SYNC_BIND").unwrap_or_else(|_|"0.0.0.0:8080".into());let listener=tokio::net::TcpListener::bind(&bind).await.map_err(|_|"bind_failed")?;
   println!("Synchronization service listening; TLS must be provided by its reverse proxy.");
   axum::serve(listener,router(store).into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown()).await.map_err(|_|"serve_failed")?;
  },
  _=>return Err("usage: asa-sync-server [serve|init [password-file]|rotate-password [password-file]|backup FILE|restore FILE|status]".into()),
 }
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
