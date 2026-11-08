pub fn script() -> &'static str {
    r#"# Ark session auto-save hooks
autoload -Uz add-zsh-hook
typeset -g _ARK_NEXT_CHECK=0

ark_preexec() {
  [[ -o interactive ]] || return 0
  local -a words
  words=("${(@Q)${(z)1}}")
  local provider="${words[1]:t}"
  case "$provider" in
    codex|claude) ;;
    *) return 0 ;;
  esac
  local word
  for word in "${words[@]:1}"; do
    case "$word" in
      exec|e|-p|--print|--help|-h|--version|-V|login|logout|mcp|completion|update|doctor|';'|'&&'|'||'|'|'|'&'|'>'|'<') return 0 ;;
    esac
  done
  command ark _hook pre --provider "$provider" --cwd "$PWD" --pid "$$"
  _ARK_NEXT_CHECK=0
  return 0
}

ark_precmd() {
  [[ -o interactive && -f "$HOME/.ark/pending/$$.json" ]] || return 0
  (( SECONDS >= _ARK_NEXT_CHECK )) || return 0
  command ark _hook post --pid "$$"
  _ARK_NEXT_CHECK=$(( SECONDS + 2 ))
  return 0
}

_ark_session_keys() {
  reply=("${(@f)$(command ark _complete aliases 2>/dev/null)}")
}

_ark() {
  local -a commands
  commands=(
    'add:Add a provider session'
    'rm:Remove a saved session'
    'star:Star a saved session'
    'unstar:Remove a session star'
    'update:Update session metadata'
    'fwd:Forward a session to another provider'
    'list:List saved sessions'
    'ls:List saved sessions'
    'auto:Configure automatic saving'
    'init:Print shell integration'
  )

  if (( CURRENT == 2 )); then
    _describe -t commands 'Ark command' commands
    _ark_session_keys
    _describe -t sessions 'saved session' reply
    return
  fi

  case "$words[2]" in
    rm|star|unstar|update|fwd)
      if (( CURRENT == 3 )); then
        _ark_session_keys
        _describe -t sessions 'saved session' reply
      fi
      ;;
  esac
}

add-zsh-hook preexec ark_preexec
add-zsh-hook precmd ark_precmd
if (( $+functions[compdef] )); then
  compdef _ark ark
fi
"#
}
