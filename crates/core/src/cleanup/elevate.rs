//! 要管理员权限的项：交给提权以后再启动一次的 ShibaPack 来清（只做清理，不开窗口）。
//!
//! - Windows：PowerShell 的 `Start-Process -Verb RunAs` 弹一次 UAC。
//! - macOS：`osascript` 的 `with administrator privileges` 输入一次密码。
//! - Linux：只清用户自己的文件夹，不提权。
//!
//! 交给它的只有规则的 id、界面上去掉的条目和用户的设置，不交路径。提权的进程自己按规则重新量一遍
//! （普通权限读不到的这时才读得到），再按同一套保护检查清理，写好结果就退出。

use super::{clean_at, history, measure_source, owner, sources, Context, Options, Pick, Report, Scan};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const FLAG: &str = "--shibapack-clean";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    ids: Vec<String>,
    /// 界面上去掉的条目。
    skip: Vec<PathBuf>,
    options: Options,
    repo_root: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Outcome {
    report: Report,
    run: history::Run,
}

/// 以管理员身份清理这些项。用户在授权提示里点了取消时返回 `cancelled`；这个系统不支持时返回 `not_supported`。
pub fn clean_as_admin(ids: &[String], skip: Vec<PathBuf>, options: &Options, repo_root: Option<&Path>) -> Result<(Report, history::Run), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = std::env::temp_dir().join(format!("shibapack-clean-{}", crate::new_id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let plan_path = dir.join("plan.json");
    let plan = Plan { ids: ids.to_vec(), skip, options: options.clone(), repo_root: repo_root.map(Path::to_path_buf) };
    std::fs::write(&plan_path, serde_json::to_vec(&plan).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let launched = launch(&exe, &plan_path);
    let outcome = std::fs::read(dir.join("result.json")).ok().and_then(|bytes| serde_json::from_slice::<Outcome>(&bytes).ok());
    let _ = std::fs::remove_dir_all(&dir);
    match (outcome, launched) {
        (Some(outcome), _) => Ok((outcome.report, outcome.run)),
        (None, Err(e)) => Err(e),
        // 没有结果文件：多半是在授权提示里点了取消
        (None, Ok(())) => Err("cancelled".into()),
    }
}

#[cfg(windows)]
fn launch(exe: &Path, plan: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // PowerShell 的单引号字符串里，单引号写两遍；路径可能有空格，参数再包一层双引号
    let quote = |path: &Path| path.to_string_lossy().replace('\'', "''");
    let script = format!("Start-Process -FilePath '{}' -ArgumentList @('{FLAG}', '\"{}\"') -Verb RunAs -Wait", quote(exe), quote(plan));
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn launch(exe: &Path, plan: &Path) -> Result<(), String> {
    // AppleScript 的字符串里，反斜杠和双引号要转义；`quoted form of` 再交给 shell 时加好单引号
    let text = |path: &Path| format!("\"{}\"", path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""));
    let script = format!("do shell script (quoted form of {}) & \" {FLAG} \" & (quoted form of {}) with administrator privileges", text(exe), text(plan));
    let output = std::process::Command::new("osascript").args(["-e", &script]).output().map_err(|e| e.to_string())?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() || stderr.contains("-128") {
        // -128：用户点了取消，没有结果文件，上面会当作 cancelled
        Ok(())
    } else {
        Err(stderr.trim().to_string())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn launch(_exe: &Path, _plan: &Path) -> Result<(), String> {
    Err("not_supported".into())
}

/// 提权启动的那个进程：命令行是 `--shibapack-clean <清单>` 时只做清理，写好结果就退出。
/// 在应用启动的最开头调用；不是这种启动方式时返回 `None`，照常启动。
pub fn elevated_entry() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.to_string_lossy() != FLAG {
        return None;
    }
    let plan_path = PathBuf::from(args.next()?.to_string_lossy().trim_matches('"'));
    let Some(plan) = std::fs::read(&plan_path).ok().and_then(|bytes| serde_json::from_slice::<Plan>(&bytes).ok()) else {
        return Some(2);
    };
    let (report, run) = run_plan(&plan);
    let written = serde_json::to_vec(&Outcome { report, run }).ok().and_then(|body| std::fs::write(plan_path.with_file_name("result.json"), body).ok());
    Some(if written.is_some() { 0 } else { 1 })
}

/// 按规则重新量一遍这些项，再清理（去掉界面上不要的条目）。只认要管理员权限的规则。
fn run_plan(plan: &Plan) -> (Report, history::Run) {
    let mut ctx = Context::new(plan.repo_root.as_deref(), None, Vec::new(), plan.options.clone());
    ctx.elevated = true;
    let me = ctx.vars.get("home").and_then(owner);
    let now = SystemTime::now();
    let found = sources(&ctx)
        .iter()
        .filter(|source| source.filter.admin && plan.ids.contains(&source.id))
        .filter_map(|source| measure_source(source, &ctx, me, now))
        .collect();
    let scan = Scan { found };
    let picks: Vec<Pick> = plan
        .ids
        .iter()
        .map(|id| Pick {
            id: id.clone(),
            skip: scan
                .found
                .iter()
                .find(|f| &f.item.id == id)
                .map(|f| f.targets.iter().enumerate().filter(|(_, t)| plan.skip.contains(&t.path)).map(|(i, _)| i).collect())
                .unwrap_or_default(),
        })
        .collect();
    clean_at(&scan, &picks, &ctx, &|_, _| {}, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_round_trips() {
        let plan = Plan {
            ids: vec!["system.windows-temp".into()],
            skip: vec![PathBuf::from("C:/Windows/Temp/keep")],
            options: Options { keep_cookies: vec!["github.com".into()], ..Options::default() },
            repo_root: Some(PathBuf::from("/repo")),
        };
        let back: Plan = serde_json::from_slice(&serde_json::to_vec(&plan).unwrap()).unwrap();
        assert_eq!(back.ids, plan.ids);
        assert_eq!(back.skip, plan.skip);
        assert_eq!(back.options, plan.options);
    }

    #[test]
    fn non_admin_rules_are_ignored_by_the_elevated_run() {
        // 提权的进程只认要管理员权限的规则：给它普通规则的 id 也不会清
        let plan = Plan { ids: vec!["development.pip".into()], skip: Vec::new(), options: Options::default(), repo_root: None };
        let (report, _) = run_plan(&plan);
        assert_eq!(report.removed, 0);
        assert_eq!(report.freed, 0);
    }
}
