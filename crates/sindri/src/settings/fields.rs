pub(super) struct FieldView {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) group: &'static str,
    pub(super) help: &'static str,
}

pub(super) const FIELDS: [FieldView; 23] = [
    FieldView {
        key: "provider",
        label: "Wire",
        group: "CONNECTION",
        help: "Select the wire used to ask the endpoint.",
    },
    FieldView {
        key: "model",
        label: "Model",
        group: "CONNECTION",
        help: "Choose a served name; Ctrl-U in the listing enters one manually.",
    },
    FieldView {
        key: "route",
        label: "Route",
        group: "CONNECTION",
        help: "Automatic follows the model. Anthropic needs no route.",
    },
    FieldView {
        key: "base_url",
        label: "Base URL",
        group: "CONNECTION",
        help: "Set an alternate endpoint address (base_url).",
    },
    FieldView {
        key: "api_key",
        label: "Credential",
        group: "CONNECTION",
        help: "Kept hidden, not verified. Choose Replace or Remove explicitly.",
    },
    FieldView {
        key: "extra_headers",
        label: "Extra headers",
        group: "CONNECTION",
        help: "Additional request headers (extra_headers).",
    },
    FieldView {
        key: "responses_continuation",
        label: "Responses continuation",
        group: "RUN & LIMITS",
        help: "Use stored response handles instead of full replay.",
    },
    FieldView {
        key: "responses_store",
        label: "Responses storage",
        group: "RUN & LIMITS",
        help: "Explicitly allow or disable server response storage.",
    },
    FieldView {
        key: "responses_continuation_verified",
        label: "Verified continuation",
        group: "RUN & LIMITS",
        help: "Assert custom endpoint storage and chaining were verified.",
    },
    FieldView {
        key: "native_compaction",
        label: "Native compaction",
        group: "RUN & LIMITS",
        help: "Explicit standalone provider compaction; incompatible with chains.",
    },
    FieldView {
        key: "native_compaction_verified",
        label: "Verified compaction",
        group: "RUN & LIMITS",
        help: "Assert this exact endpoint and model support the documented contract.",
    },
    FieldView {
        key: "thinking",
        label: "Thinking",
        group: "RUN & LIMITS",
        help: "Enable model thinking where the wire supports it.",
    },
    FieldView {
        key: "context_window",
        label: "Context window",
        group: "RUN & LIMITS",
        help: "Declare the model window in tokens (context_window).",
    },
    FieldView {
        key: "output_headroom",
        label: "Output headroom",
        group: "RUN & LIMITS",
        help: "Reserve tokens for the model output.",
    },
    FieldView {
        key: "tool_result_headroom",
        label: "Tool result headroom",
        group: "RUN & LIMITS",
        help: "Reserve tokens for expected tool results.",
    },
    FieldView {
        key: "max_turns",
        label: "Maximum turns",
        group: "RUN & LIMITS",
        help: "Stop after this many model turns (max_turns).",
    },
    FieldView {
        key: "timeout",
        label: "Request timeout",
        group: "RUN & LIMITS",
        help: "Request timeout in seconds (timeout).",
    },
    FieldView {
        key: "command_budget_ms",
        label: "Command budget",
        group: "RUN & LIMITS",
        help: "Command budget in milliseconds (command_budget_ms).",
    },
    FieldView {
        key: "output_budget_bytes",
        label: "Output budget",
        group: "RUN & LIMITS",
        help: "Command output budget in bytes (output_budget_bytes).",
    },
    FieldView {
        key: "extra_writable_paths",
        label: "Extra writable paths",
        group: "ACCESS & APPEARANCE",
        help: "Additional writable paths (extra_writable_paths).",
    },
    FieldView {
        key: "ssh_agent",
        label: "SSH agent",
        group: "ACCESS & APPEARANCE",
        help: "Allow the SSH agent inside confinement (ssh_agent).",
    },
    FieldView {
        key: "status",
        label: "Status reporting",
        group: "ACCESS & APPEARANCE",
        help: "Report status to the terminal (status).",
    },
    FieldView {
        key: "face",
        label: "Sindri's face",
        group: "ACCESS & APPEARANCE",
        help: "Show Sindri's face where the terminal can place it (face).",
    },
];

pub(super) fn grouped(value: &str) -> String {
    if value.parse::<u64>().is_err() {
        return value.to_owned();
    }
    let mut result = String::new();
    for (at, glyph) in value.chars().enumerate() {
        if at > 0 && (value.len() - at).is_multiple_of(3) {
            result.push(',');
        }
        result.push(glyph);
    }
    result
}
