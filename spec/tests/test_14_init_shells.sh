# Init command shell function tests
# Spec: init_spec.md

section "init-shells"

# Test: init with bash shell emits bash function
output=$(SHELL=/bin/bash try_run init "$TEST_TRIES" 2>&1)
if echo "$output" | grep -q "try() {"; then
    pass
else
    fail "init should emit bash function" "try() {" "$output" "init_spec.md"
fi

# Test: bash function includes --path argument with the specified path
if echo "$output" | grep -qF -- "--path '$TEST_TRIES'"; then
    pass
else
    fail "bash function should include --path with specified path" "--path '$TEST_TRIES'" "$output" "init_spec.md"
fi

# Test: init with fish shell emits fish function
output=$(SHELL=/usr/bin/fish try_run init "$TEST_TRIES" 2>&1)
if echo "$output" | grep -q "function try"; then
    pass
else
    fail "init with fish should emit fish function" "function try" "$output" "init_spec.md"
fi

# Test: init output contains the real, full path to try binary
output=$(SHELL=/bin/bash try_run init "$TEST_TRIES" 2>&1)
if echo "$output" | grep -qF "$TRY_BIN_PATH"; then
    pass
else
    fail "init should contain real, full path to try binary" "$TRY_BIN_PATH" "$output" "init_spec.md"
fi

# Test: init detects PowerShell and emits a pwsh function, matching `install`
# (init used to only ever choose between fish and bash).
output=$(SHELL= PSModulePath='C:\Program Files\PowerShell\Modules' try_run init "$TEST_TRIES" 2>&1)
if echo "$output" | grep -q "function try {"; then
    pass
else
    fail "init with PowerShell should emit a pwsh function" "function try {" "$output" "init_spec.md"
fi

# Test: the pwsh function invokes try and evaluates its output
if echo "$output" | grep -q "Invoke-Expression"; then
    pass
else
    fail "pwsh function should eval try output" "Invoke-Expression" "$output" "init_spec.md"
fi

# Test: zsh gets the POSIX function (bash and zsh share one snippet)
output=$(SHELL=/bin/zsh try_run init "$TEST_TRIES" 2>&1)
if echo "$output" | grep -q "try() {"; then
    pass
else
    fail "init with zsh should emit a POSIX function" "try() {" "$output" "init_spec.md"
fi
