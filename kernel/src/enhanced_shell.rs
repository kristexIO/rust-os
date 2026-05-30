use crate::fast_terminal::Terminal;
use crate::graphics::DrawTarget;
use crate::vfs::{self, FileSystem};
use alloc::{format, string::String, string::ToString, vec::Vec};

pub struct EnhancedShell {
    pub history: Vec<String>,
    pub current_dir: String,
    pub env_vars: alloc::collections::BTreeMap<String, String>,
}

impl EnhancedShell {
    pub fn new() -> Self {
        let mut shell = EnhancedShell {
            history: Vec::new(),
            current_dir: "/".to_string(),
            env_vars: alloc::collections::BTreeMap::new(),
        };

        // Установим базовые переменные окружения
        shell
            .env_vars
            .insert("PATH".to_string(), "/bin:/usr/bin".to_string());
        shell
            .env_vars
            .insert("HOME".to_string(), "/home/user".to_string());
        shell.env_vars.insert("PWD".to_string(), "/".to_string());

        shell
    }

    pub fn run_command(&mut self, command: &str, term: &mut Terminal, target: &mut dyn DrawTarget) {
        let trimmed_cmd = command.trim();
        if trimmed_cmd.is_empty() {
            return;
        }

        // Добавляем команду в историю
        self.history.push(trimmed_cmd.to_string());

        let parts: Vec<&str> = trimmed_cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        let cmd = parts[0].to_lowercase();
        let args = &parts[1..];

        match cmd.as_str() {
            "help" => self.show_help(term, target),
            "echo" => self.echo_command(args, term, target),
            "ls" => self.ls_command(args, term, target),
            "cd" => self.cd_command(args, term, target),
            "pwd" => self.pwd_command(term, target),
            "mkdir" => self.mkdir_command(args, term, target),
            "touch" => self.touch_command(args, term, target),
            "cat" => self.cat_command(args, term, target),
            "rm" => self.rm_command(args, term, target),
            "history" => self.history_command(term, target),
            "env" => self.env_command(term, target),
            "set" => self.set_env_command(args, term, target),
            "unset" => self.unset_env_command(args, term, target),
            "ps" => self.ps_command(term, target), // Виртуальная команда пока
            "df" => self.df_command(term, target), // Виртуальная команда пока
            "clear" => {
                // Очистка терминала (в реальной системе это должно очищать экран)
                term.reset_cursor();
            }
            _ => {
                term.print_str(target, &format!("Command '{}' not found\n", cmd));
                term.print_str(target, "Type 'help' for available commands\n");
            }
        }
    }

    fn show_help(&self, term: &mut Terminal, target: &mut dyn DrawTarget) {
        term.print_str(target, "Available commands:\n");
        term.print_str(target, "  help          - Show this help\n");
        term.print_str(target, "  echo <text>   - Print text\n");
        term.print_str(target, "  ls [dir]      - List directory contents\n");
        term.print_str(target, "  cd <dir>      - Change directory\n");
        term.print_str(target, "  pwd           - Print working directory\n");
        term.print_str(target, "  mkdir <dir>   - Create directory\n");
        term.print_str(
            target,
            "  touch <file>  - Create or update file timestamp\n",
        );
        term.print_str(target, "  cat <file>    - Display file contents\n");
        term.print_str(target, "  rm <file>     - Remove file/directory\n");
        term.print_str(target, "  history       - Show command history\n");
        term.print_str(target, "  env           - Show environment variables\n");
        term.print_str(target, "  set VAR VAL   - Set environment variable\n");
        term.print_str(target, "  unset VAR     - Unset environment variable\n");
        term.print_str(target, "  ps            - Show running processes\n");
        term.print_str(target, "  df            - Show disk usage\n");
        term.print_str(target, "  clear         - Clear screen\n");
    }

    fn echo_command(&self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.is_empty() {
            term.print_str(target, "\n");
            return;
        }

        let mut output = String::new();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                output.push(' ');
            }
            // Проверяем, является ли аргумент переменной окружения (вида $VAR)
            if arg.starts_with('$') {
                let var_name = &arg[1..];
                if let Some(value) = self.env_vars.get(var_name) {
                    output.push_str(value);
                } else {
                    output.push_str(arg);
                }
            } else {
                output.push_str(arg);
            }
        }
        output.push('\n');
        term.print_str(target, &output);
    }

    fn ls_command(&self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        let path = if args.is_empty() {
            self.current_dir.clone()
        } else {
            self.resolve_path(args[0])
        };

        match vfs::GLOBAL_VFS.lock().read_dir(&path) {
            Ok(entries) => {
                for entry in entries {
                    term.print_str(target, &format!("{}\n", entry.name));
                }
            }
            Err(e) => {
                term.print_str(target, &format!("ls: {}: {:?}\n", path, e));
            }
        }
    }

    fn cd_command(&mut self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.is_empty() {
            term.print_str(target, "cd: missing operand\n");
            return;
        }

        let new_path = self.resolve_path(args[0]);

        // Проверяем, существует ли директория
        // Используем read_dir для проверки существования
        match vfs::GLOBAL_VFS.lock().read_dir(&new_path) {
            Ok(_) => {
                self.current_dir = new_path;
                self.update_pwd_env();
                term.print_str(
                    target,
                    &format!("Changed directory to: {}\n", self.current_dir),
                );
            }
            Err(_) => {
                // Если директория не существует, проверим, может быть это файл?
                match vfs::GLOBAL_VFS.lock().read_file(&new_path) {
                    Ok(_) => {
                        term.print_str(target, &format!("cd: {}: Not a directory\n", args[0]));
                    }
                    Err(_) => {
                        term.print_str(
                            target,
                            &format!("cd: {}: No such file or directory\n", args[0]),
                        );
                    }
                }
            }
        }
    }

    fn pwd_command(&self, term: &mut Terminal, target: &mut dyn DrawTarget) {
        term.print_str(target, &format!("{}\n", self.current_dir));
    }

    fn mkdir_command(&self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.is_empty() {
            term.print_str(target, "mkdir: missing operand\n");
            return;
        }

        let path = self.resolve_path(args[0]);

        match vfs::GLOBAL_VFS.lock().mkdir(&path) {
            Ok(()) => {
                term.print_str(target, &format!("Created directory: {}\n", path));
            }
            Err(e) => {
                term.print_str(target, &format!("mkdir: {}: {:?}\n", args[0], e));
            }
        }
    }

    fn touch_command(&self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.is_empty() {
            term.print_str(target, "touch: missing operand\n");
            return;
        }

        let path = self.resolve_path(args[0]);

        // Попробуем создать файл если он не существует
        match vfs::GLOBAL_VFS.lock().create_file(&path) {
            Ok(()) => {
                term.print_str(target, &format!("Created/updating file: {}\n", path));
            }
            Err(e) => {
                term.print_str(target, &format!("touch: {}: {:?}\n", args[0], e));
            }
        }
    }

    fn cat_command(&self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.is_empty() {
            term.print_str(target, "cat: missing operand\n");
            return;
        }

        let path = self.resolve_path(args[0]);

        match vfs::GLOBAL_VFS.lock().read_file(&path) {
            Ok(content) => {
                if let Ok(text) = core::str::from_utf8(&content) {
                    term.print_str(target, text);
                } else {
                    term.print_str(target, &format!("cat: {}: Binary file\n", path));
                }
            }
            Err(e) => {
                term.print_str(target, &format!("cat: {}: {:?}\n", args[0], e));
            }
        }
    }

    fn rm_command(&self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.is_empty() {
            term.print_str(target, "rm: missing operand\n");
            return;
        }

        let path = self.resolve_path(args[0]);

        match vfs::GLOBAL_VFS.lock().remove(&path) {
            Ok(()) => {
                term.print_str(target, &format!("Removed: {}\n", path));
            }
            Err(e) => {
                term.print_str(target, &format!("rm: {}: {:?}\n", args[0], e));
            }
        }
    }

    fn history_command(&self, term: &mut Terminal, target: &mut dyn DrawTarget) {
        for (i, command) in self.history.iter().enumerate() {
            term.print_str(target, &format!("{:3}: {}\n", i, command));
        }
    }

    fn env_command(&self, term: &mut Terminal, target: &mut dyn DrawTarget) {
        for (key, value) in &self.env_vars {
            term.print_str(target, &format!("{}={}\n", key, value));
        }
    }

    fn set_env_command(&mut self, args: &[&str], term: &mut Terminal, target: &mut dyn DrawTarget) {
        if args.len() < 2 {
            term.print_str(target, "set: usage: set VAR VALUE\n");
            return;
        }

        let var_name = args[0];
        let var_value = args[1..].join(" ");

        self.env_vars.insert(var_name.to_string(), var_value);
        term.print_str(
            target,
            &format!("Set {}={}\n", var_name, args[1..].join(" ")),
        );
    }

    fn unset_env_command(
        &mut self,
        args: &[&str],
        term: &mut Terminal,
        target: &mut dyn DrawTarget,
    ) {
        if args.is_empty() {
            term.print_str(target, "unset: usage: unset VAR\n");
            return;
        }

        let var_name = args[0];

        if self.env_vars.remove(var_name).is_some() {
            term.print_str(target, &format!("Unset {}\n", var_name));
        } else {
            term.print_str(target, &format!("unset: {}: not set\n", var_name));
        }
    }

    fn ps_command(&self, term: &mut Terminal, target: &mut dyn DrawTarget) {
        term.print_str(target, "PID    Name          Status\n");
        term.print_str(target, "----   ----          ------\n");
        term.print_str(target, "1      init          running\n");
        term.print_str(target, "2      shell         running\n");
        term.print_str(target, "3      compositor    sleeping\n");
    }

    fn df_command(&self, term: &mut Terminal, target: &mut dyn DrawTarget) {
        term.print_str(target, "Filesystem     Size  Used Avail Use% Mounted on\n");
        term.print_str(target, "/dev/sda1      10M   2.0M  8.0M  20% /\n");
        term.print_str(target, "/dev/sdb       100M  10M   90M   10% /mnt\n");
    }

    fn resolve_path(&self, path: &str) -> String {
        if path.starts_with('/') {
            // Абсолютный путь
            path.to_string()
        } else {
            // Относительный путь
            if self.current_dir.ends_with('/') {
                format!("{}{}", self.current_dir, path)
            } else {
                format!("{}/{}", self.current_dir, path)
            }
        }
    }

    fn update_pwd_env(&mut self) {
        self.env_vars
            .insert("PWD".to_string(), self.current_dir.clone());
    }
}
