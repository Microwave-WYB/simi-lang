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

        let command = worktree.which("simi").ok_or_else(|| {
            "simi was not found on the worktree PATH; build or install it into a PATH directory, then run the language server with `simi lsp`"
                .to_owned()
        })?;

        Ok(zed::Command {
            command,
            args: vec!["lsp".to_owned()],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(SimiExtension);
