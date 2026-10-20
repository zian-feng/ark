pub fn script() -> &'static str {
    r#"# Ark session auto-save hooks
autoload -Uz add-zsh-hook

ark_preexec() {
  [[ -o interactive ]] || return
  case "$1" in
    codex|codex\ *)
      [[ "$1" == "codex exec"* ]] || command ark _hook pre --provider codex --cwd "$PWD" --pid "$$"
      ;;
    claude|claude\ *)
      command ark _hook pre --provider claude --cwd "$PWD" --pid "$$"
      ;;
  esac
}

ark_precmd() {
  [[ -o interactive ]] || return
  command ark _hook post --pid "$$"
}

add-zsh-hook preexec ark_preexec
add-zsh-hook precmd ark_precmd
"#
}
