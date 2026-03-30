-- selfware-codemap.lua
-- Neovim plugin for browsing and selecting nodes from codegraph.json.
--
-- Requirements: telescope.nvim
-- Install: symlink or copy this file into ~/.config/nvim/lua/selfware-codemap.lua
-- then `require("selfware-codemap").setup()` in your init.lua.

local M = {}

-- ---------------------------------------------------------------------------
-- State
-- ---------------------------------------------------------------------------

--- Parsed codegraph (table).  Loaded lazily on first use.
M._graph = nil

--- Context set: map of node_id -> node table.
M._context = {}

--- Token budget (configurable).
M._budget = 128000

-- ---------------------------------------------------------------------------
-- Graph loading
-- ---------------------------------------------------------------------------

--- Find codegraph.json relative to the workspace root.
local function graph_path()
    local root = vim.fn.getcwd()
    return root .. "/codegraph.json"
end

--- (Re)load codegraph.json into M._graph.
function M.load_graph()
    local path = graph_path()
    local ok, lines = pcall(vim.fn.readfile, path)
    if not ok then
        vim.notify("selfware-codemap: cannot read " .. path, vim.log.levels.WARN)
        return
    end
    local raw = table.concat(lines, "\n")
    local parsed = vim.fn.json_decode(raw)
    if parsed then
        M._graph = parsed
    else
        vim.notify("selfware-codemap: failed to parse codegraph.json", vim.log.levels.ERROR)
    end
end

--- Return the nodes list from the graph, loading if needed.
local function nodes()
    if not M._graph then
        M.load_graph()
    end
    return (M._graph or {}).nodes or {}
end

-- ---------------------------------------------------------------------------
-- Context tracking
-- ---------------------------------------------------------------------------

--- Total tokens currently in the context set.
local function context_tokens()
    local total = 0
    for _, node in pairs(M._context) do
        total = total + (node.tokens or 0)
    end
    return total
end

--- Add a node to the context set.
function M.context_add(node)
    if not node or not node.id then return end
    M._context[node.id] = node
    vim.notify(string.format("+ %s (%d tok) | budget %d/%d",
        node.id, node.tokens or 0, context_tokens(), M._budget))
end

--- Remove a node from the context set.
function M.context_remove(node_id)
    if M._context[node_id] then
        M._context[node_id] = nil
        vim.notify(string.format("- %s | budget %d/%d",
            node_id, context_tokens(), M._budget))
    end
end

-- ---------------------------------------------------------------------------
-- Inspect (floating window)
-- ---------------------------------------------------------------------------

function M.inspect(node)
    if not node then return end

    local lines = {
        "Node: " .. (node.id or "?"),
        "Kind: " .. (node.kind or "?"),
        "File: " .. (node.file or "?"),
        "Tokens: " .. tostring(node.tokens or 0),
        "",
        "Dependencies:",
    }
    for _, dep in ipairs(node.deps or {}) do
        table.insert(lines, "  -> " .. dep)
    end

    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)

    local width = 60
    local height = #lines
    local row = math.floor((vim.o.lines - height) / 2)
    local col = math.floor((vim.o.columns - width) / 2)

    vim.api.nvim_open_win(buf, true, {
        relative = "editor",
        width = width,
        height = height,
        row = row,
        col = col,
        style = "minimal",
        border = "rounded",
    })
end

-- ---------------------------------------------------------------------------
-- Telescope picker
-- ---------------------------------------------------------------------------

function M.open_codemap()
    local ok, pickers = pcall(require, "telescope.pickers")
    if not ok then
        vim.notify("selfware-codemap: telescope.nvim required", vim.log.levels.ERROR)
        return
    end
    local finders = require("telescope.finders")
    local conf = require("telescope.config").values
    local actions = require("telescope.actions")
    local action_state = require("telescope.actions.state")

    local items = {}
    for _, node in ipairs(nodes()) do
        table.insert(items, {
            display = string.format("[%s] %s  (%d tok)",
                node.kind or "?", node.id or "?", node.tokens or 0),
            node = node,
            ordinal = node.id or "",
        })
    end

    pickers.new({}, {
        prompt_title = "Selfware Code Map",
        finder = finders.new_table({
            results = items,
            entry_maker = function(item)
                return {
                    value = item,
                    display = item.display,
                    ordinal = item.ordinal,
                }
            end,
        }),
        sorter = conf.generic_sorter({}),
        attach_mappings = function(prompt_bufnr, map)
            -- Enter = inspect
            actions.select_default:replace(function()
                local sel = action_state.get_selected_entry()
                actions.close(prompt_bufnr)
                if sel then M.inspect(sel.value.node) end
            end)
            -- <C-a> = add to context
            map("i", "<C-a>", function()
                local sel = action_state.get_selected_entry()
                if sel then M.context_add(sel.value.node) end
            end)
            -- <C-d> = remove from context
            map("i", "<C-d>", function()
                local sel = action_state.get_selected_entry()
                if sel then M.context_remove(sel.value.node.id) end
            end)
            return true
        end,
    }):find()
end

-- ---------------------------------------------------------------------------
-- Statusline component
-- ---------------------------------------------------------------------------

--- Returns a string suitable for lualine or a manual statusline.
function M.statusline()
    local used = context_tokens()
    return string.format("ctx %d/%d tok", used, M._budget)
end

-- ---------------------------------------------------------------------------
-- Setup
-- ---------------------------------------------------------------------------

function M.setup(opts)
    opts = opts or {}
    M._budget = opts.budget or M._budget

    -- Keybindings (normal mode, buffer-local = false)
    vim.keymap.set("n", "<leader>cm", M.open_codemap, { desc = "Code Map: open" })
    vim.keymap.set("n", "<leader>ca", function()
        -- Quick-add: find the node whose file matches the current buffer
        local file = vim.fn.expand("%:p")
        for _, node in ipairs(nodes()) do
            if node.file and vim.endswith(file, node.file) then
                M.context_add(node)
                return
            end
        end
        vim.notify("selfware-codemap: no graph node for this file", vim.log.levels.INFO)
    end, { desc = "Code Map: add to context" })
    vim.keymap.set("n", "<leader>cr", function()
        local file = vim.fn.expand("%:p")
        for _, node in ipairs(nodes()) do
            if node.file and vim.endswith(file, node.file) then
                M.context_remove(node.id)
                return
            end
        end
    end, { desc = "Code Map: remove from context" })
    vim.keymap.set("n", "<leader>ci", function()
        local file = vim.fn.expand("%:p")
        for _, node in ipairs(nodes()) do
            if node.file and vim.endswith(file, node.file) then
                M.inspect(node)
                return
            end
        end
        vim.notify("selfware-codemap: no graph node for this file", vim.log.levels.INFO)
    end, { desc = "Code Map: inspect" })
end

return M
