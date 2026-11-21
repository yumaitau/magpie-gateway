--[[
Pandoc Lua filter for branded Magpie Gateway Word (.docx) deliverables.
Adds Magpie cover page, Document Control section, live Word TOC field with PAGEREF bookmarks,
content-weighted table widths, numbered Figure/Table captions, and callout styles.
]]

local stringify = pandoc.utils.stringify

local function raw(xml) return pandoc.RawBlock("openxml", xml) end

local function esc(s)
  return (s:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"))
end

local PAGE_BREAK = raw('<w:p><w:r><w:br w:type="page"/></w:r></w:p>')

local function Div(div)
  local c = div.classes
  if c:includes("important") or c:includes("warning") then
    div.attributes["custom-style"] = "Callout Important"
    div.classes = {}
    return div
  elseif c:includes("note") then
    div.attributes["custom-style"] = "Note"
    div.classes = {}
    return div
  end
end

local function Figure(fig)
  return pandoc.walk_block(fig, {
    Image = function(img)
      if not img.attributes.width then img.attributes.width = "100%" end
      return img
    end,
  })
end

local function Table(tbl)
  local n = #tbl.colspecs
  if n == 0 then return nil end
  local maxlen, total = {}, {}
  local rows = 0
  local function measure(row)
    for i, cell in ipairs(row.cells) do
      if i <= n then
        local len = utf8.len(stringify(cell.contents)) or 0
        maxlen[i] = math.max(maxlen[i] or 0, len)
        total[i] = (total[i] or 0) + len
      end
    end
  end
  for _, row in ipairs(tbl.head.rows) do measure(row) end
  for _, body in ipairs(tbl.bodies) do
    for _, row in ipairs(body.body) do
      measure(row)
      rows = rows + 1
    end
  end
  local weights, sum = {}, 0
  for i = 1, n do
    local avg = (total[i] or 0) / math.max(rows + 1, 1)
    local w = math.max(4, 0.6 * math.min(maxlen[i] or 0, 60) + 0.4 * avg)
    weights[i] = math.sqrt(w)
    sum = sum + weights[i]
  end
  for i = 1, n do
    local share = math.max(weights[i] / sum, 0.08)
    tbl.colspecs[i] = { tbl.colspecs[i][1], share }
  end
  local s2 = 0
  for i = 1, n do s2 = s2 + tbl.colspecs[i][2] end
  for i = 1, n do tbl.colspecs[i] = { tbl.colspecs[i][1], tbl.colspecs[i][2] / s2 } end
  return tbl
end

local function caption_label(kind, n)
  return pandoc.RawInline("openxml", '<w:r><w:t xml:space="preserve">' .. kind .. ' </w:t></w:r>'
    .. '<w:fldSimple w:instr=" SEQ ' .. kind .. ' \\* ARABIC "><w:r><w:t>' .. n .. '</w:t></w:r></w:fldSimple>'
    .. '<w:r><w:t xml:space="preserve">. </w:t></w:r>')
end

local function prefix_caption(caption, kind, n)
  local first = caption.long[1]
  if not first or #stringify(caption.long) == 0 then return false end
  if first.t == "Plain" or first.t == "Para" then
    first.content:insert(1, caption_label(kind, n))
  else
    caption.long:insert(1, pandoc.Plain({ caption_label(kind, n) }))
  end
  return true
end

local function number_captions(blocks)
  local figures, tables = 0, 0
  return blocks:walk({
    Figure = function(fig)
      if #stringify(fig.caption.long) > 0 then
        figures = figures + 1
        prefix_caption(fig.caption, "Figure", figures)
        return fig
      end
    end,
    Table = function(tbl)
      if #stringify(tbl.caption.long) > 0 then
        tables = tables + 1
        prefix_caption(tbl.caption, "Table", tables)
        return tbl
      end
    end,
  })
end

local function cover(meta)
  local blocks = {}
  table.insert(blocks, pandoc.Para({ pandoc.Image("Magpie Gateway emblem", "magpie-emblem.png", "", { width = "3.2cm" }) }))
  table.insert(blocks, pandoc.Div({ pandoc.Para(pandoc.Inlines(meta.kicker and stringify(meta.kicker) or "Enterprise & Government Guide")) }, { ["custom-style"] = "Cover Kicker" }))
  table.insert(blocks, pandoc.Div({ pandoc.Para({ pandoc.Str("Magpie Gateway") }) }, { ["custom-style"] = "Title" }))
  table.insert(blocks, pandoc.Div({ pandoc.Para(pandoc.Inlines(meta.subtitle and stringify(meta.subtitle) or "Sovereign AI Governance, Australian Ready")) }, { ["custom-style"] = "Subtitle" }))
  table.insert(blocks, pandoc.Div({ pandoc.Para(pandoc.Inlines(meta.note and stringify(meta.note) or "For Microsoft Entra ID, Intune, Microsoft Sentinel, Azure and Australian Government")) }, { ["custom-style"] = "Cover Note" }))
  local rows = {
    { "Document", stringify(meta.title) },
    { "Version", stringify(meta.version) },
    { "Date", stringify(meta.date) },
    { "Status", stringify(meta.status) },
    { "Platform", "Magpie Gateway (built on Agentgateway)" },
  }
  local md = "| Item | Value |\n|---|---|\n"
  for _, r in ipairs(rows) do md = md .. "| " .. r[1] .. " | " .. r[2] .. " |\n" end
  for _, b in ipairs(pandoc.read(md, "markdown").blocks) do table.insert(blocks, b) end
  table.insert(blocks, PAGE_BREAK)
  return blocks
end

local function run(text) return '<w:r><w:t xml:space="preserve">' .. esc(text) .. "</w:t></w:r>" end

local function toc_entry(level, anchor, label, first, last)
  local xml = '<w:p><w:pPr><w:pStyle w:val="TOC' .. level .. '"/></w:pPr>'
  if first then
    xml = xml .. '<w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r>'
      .. '<w:r><w:instrText xml:space="preserve"> TOC \\o "1-2" \\h \\z \\u </w:instrText></w:r>'
      .. '<w:r><w:fldChar w:fldCharType="separate"/></w:r>'
  end
  xml = xml .. '<w:hyperlink w:anchor="' .. anchor .. '" w:history="1">' .. run(label) .. "<w:r><w:tab/></w:r>"
    .. '<w:r><w:fldChar w:fldCharType="begin"/></w:r>'
    .. '<w:r><w:instrText xml:space="preserve"> PAGEREF ' .. anchor .. ' \\h </w:instrText></w:r>'
    .. '<w:r><w:fldChar w:fldCharType="separate"/></w:r>' .. run("")
    .. '<w:r><w:fldChar w:fldCharType="end"/></w:r></w:hyperlink>'
  if last then xml = xml .. '<w:r><w:fldChar w:fldCharType="end"/></w:r>' end
  return xml .. "</w:p>"
end

function Pandoc(doc)
  local n1, n2, n3 = 0, 0, 0
  local entries = {}
  doc.blocks = doc.blocks:walk({
    Header = function(h)
      if h.classes:includes("unnumbered") then return nil end
      if h.level == 1 then
        n1, n2, n3 = n1 + 1, 0, 0
        h.identifier = "sec" .. n1
        table.insert(entries, { 1, h.identifier, n1 .. "  " .. stringify(h.content) })
      elseif h.level == 2 then
        n2, n3 = n2 + 1, 0
        h.identifier = "sec" .. n1 .. "_" .. n2
        table.insert(entries, { 2, h.identifier, n1 .. "." .. n2 .. "  " .. stringify(h.content) })
      elseif h.level == 3 then
        n3 = n3 + 1
        h.identifier = "sec" .. n1 .. "_" .. n2 .. "_" .. n3
      end
      return h
    end,
  })
  doc.blocks = doc.blocks:walk({ Div = Div, Figure = Figure, Table = Table })
  doc.blocks = number_captions(doc.blocks)

  local front = cover(doc.meta)
  if doc.meta.control then
    local control = pandoc.Blocks(doc.meta.control):walk({
      Header = function(h)
        return pandoc.Div({ pandoc.Para(h.content) }, { ["custom-style"] = "Control Heading" })
      end,
      Table = Table,
    })
    for _, b in ipairs(control) do table.insert(front, b) end
  end

  table.insert(front, PAGE_BREAK)
  local toc = '<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/>'
    .. '</w:docPartObj></w:sdtPr><w:sdtContent>'
    .. '<w:p><w:pPr><w:pStyle w:val="TOCHeading"/></w:pPr>' .. run("Contents") .. "</w:p>"
  for i, e in ipairs(entries) do
    toc = toc .. toc_entry(e[1], e[2], e[3], i == 1, i == #entries)
  end
  toc = toc .. "</w:sdtContent></w:sdt>"
  table.insert(front, raw(toc))

  local blocks = pandoc.Blocks(front)
  blocks:extend(doc.blocks)
  doc.blocks = blocks
  for _, k in ipairs({ "title", "subtitle", "author", "date", "status", "version", "control", "abstract", "kicker", "note", "running-title" }) do
    doc.meta[k] = nil
  end
  return doc
end
