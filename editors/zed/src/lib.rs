use zed_extension_api as zed;

struct SimiExtension;

impl zed::Extension for SimiExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        if language_server_id.as_ref() != "simi-lsp" {
            return Err(format!(
                "unsupported Simi language server: {language_server_id}"
            ));
        }

        let command = worktree.which("simi-lsp").ok_or_else(|| {
            "simi-lsp was not found on the worktree PATH; install it into a PATH directory or configure the environment used to launch Zed"
                .to_owned()
        })?;

        Ok(zed::Command {
            command,
            args: Vec::new(),
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(SimiExtension);
