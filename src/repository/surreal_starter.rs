use tokio::task;
use std::process::Command;
use std::io::Result as ResultIO;

pub  async fn database_starter() ->task::JoinHandle<()>{
    let surreal_handle = task::spawn_blocking(|| {
        // match command_line_database_starter() {
        //     Ok(child) => {
        //         println!("🚀 SurrealDB started with PID: {}", child.id());
        //     },
        //     Err(e) => eprintln!("❌ SurrealDB error: {}", e),
        // }
    });
    let _= command_line_database_starter().await;
    // Wait a moment to ensure SurrealDB is up.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    surreal_handle
}

async fn command_line_database_starter() -> ResultIO<()> {//Child
    let command =  "surreal start --user root --pass root --bind 127.0.0.1:8000";
    let _ = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args([
                "/C",
                command,
            ])
            .spawn()
    } else {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .spawn()
    };
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    Ok(())
}


// pub async fn command_line_database_starter() -> ResultIO<()> {
//     // 1. Spawn the process via tokio::process::Command
//     let child = if cfg!(target_os = "windows") {
//         Command::new("cmd")
//             .args(["/C", "surreal start --user root --pass root --bind 127.0.0.1:8000"])
//             .spawn()?
//     } else {
//         Command::new("sh")
//             .arg("-c")
//             .arg("surreal start --user root --pass root --bind 127.0.0.1:8000")
//             .spawn()?
//     };
//     println!("🚀 SurrealDB started, pid={}", child.id());
//     // 2. Store it in our global
//     let mut guard = DB_HANDLE.lock().await;
//     *guard = Some(child);
//     // 3. Give the DB a moment to come up
//     Ok(())
// }
// pub async fn stop_surrealdb() {
//     let mut guard = DB_HANDLE.lock().await;
//     if let Some(child) = guard.as_mut() {
//         println!("🛑 Killing SurrealDB (pid={})", child.id());
//         // 1. Send kill
//         child.kill().unwrap();
//         // 2. Await full shutdown
//         let status = child.wait();
//         println!("✅ SurrealDB exited with status {:?}", status);
//         // 3. Clear the handle
//         *guard = None;
//     } else {
//         println!("⚠️  No SurrealDB instance to stop");
//     }
// }