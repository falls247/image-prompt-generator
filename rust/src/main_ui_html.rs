pub fn build_main_ui_html() -> String {
    MAIN_UI_HTML.to_string()
}

const MAIN_UI_HTML: &str = r#"<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Image Prompt Generator</title>
  <style>
    :root {
      --bg: #1f2024;
      --panel: #1b1c20;
      --line: #3f4248;
      --input-bg: #272a2f;
      --input-line: #4a4e55;
      --text: #f3f5f7;
      --muted: #9ca2ad;
      --btn-bg: #2a2d33;
      --btn-line: #5b616d;
      --grid-cols: 170px 320px 44px 1fr;
      --grid-gap: 6px;
      --ctrl-h: 26px;
      --delete-h: 24px;
      --font-sm: 12px;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      color: var(--text);
      background: var(--bg);
      font-family: "Yu Gothic UI", "Hiragino Kaku Gothic ProN", sans-serif;
      font-size: 14px;
    }
    .wrap {
      width: 100%;
      height: 100vh;
      padding: 6px;
    }
    .frame {
      border: 1px solid var(--line);
      background: var(--panel);
      padding: 3px 6px 5px;
      width: 100%;
      height: 100%;
      display: flex;
      flex-direction: column;
      min-height: 0;
    }
    .top-pane {
      flex: 1 1 auto;
      min-height: 0;
      display: flex;
      flex-direction: column;
    }
    .bottom-pane {
      flex: 0 0 auto;
      border-top: 1px solid #2f3137;
      padding-top: 4px;
    }
    .grid-header, .row {
      display: grid;
      grid-template-columns: var(--grid-cols);
      gap: var(--grid-gap);
      align-items: center;
    }
    .grid-header {
      color: #ffffff;
      font-weight: 600;
      font-size: 15px;
      text-align: center;
      padding: 0 4px 2px;
      border-bottom: 1px solid #2f3137;
    }
    .grid-header > div {
      min-height: var(--ctrl-h);
      display: flex;
      align-items: center;
      justify-content: center;
      text-align: center;
    }
    #rows {
      flex: 1 1 auto;
      min-height: 0;
      overflow: auto;
      border-left: 1px solid #2f3137;
      border-right: 1px solid #2f3137;
      border-bottom: 1px solid #2f3137;
      padding: 2px 4px 1px;
      scrollbar-color: #5d6470 #25272b;
    }
    .row {
      padding: 0 2px;
      margin-bottom: 0;
    }
    .label {
      color: #ffffff;
      font-weight: 600;
      font-size: var(--font-sm);
      display: flex;
      align-items: center;
      justify-content: center;
      text-align: center;
      min-height: var(--ctrl-h);
      overflow-wrap: anywhere;
    }
    select, input, button {
      font: inherit;
    }
    select, input {
      width: 100%;
      height: var(--ctrl-h);
      border: 1px solid var(--input-line);
      background: var(--input-bg);
      padding: 0 5px;
      border-radius: 4px;
      color: var(--text);
      outline: none;
      font-size: var(--font-sm);
      line-height: 1.1;
      min-height: 0;
    }
    select {
      padding-right: 16px;
    }
    select:focus, input:focus {
      border-color: #6f8099;
    }
    input:disabled {
      background: #24262a;
      color: #7a8089;
    }
    .delete {
      width: 100%;
      height: var(--delete-h);
      border: 1px solid var(--input-line);
      border-radius: 4px;
      color: #d9dee6;
      background: #2b2e34;
      cursor: pointer;
      font-size: 9px;
      line-height: 1;
      padding: 0;
    }
    .delete:disabled {
      opacity: 0.35;
      cursor: default;
    }
    .preview-title {
      margin: 0 0 2px;
      font-size: 12px;
      color: #ffffff;
    }
    .preview {
      min-height: 108px;
      border: 1px solid #5b5f67;
      background: #1a1b1f;
      padding: 8px 9px;
      white-space: pre-wrap;
      word-break: break-word;
      color: #ffffff;
      font-size: 13px;
      line-height: 1.3;
    }
    .actions {
      margin-top: 4px;
      display: flex;
      gap: 6px;
      justify-content: space-between;
      align-items: center;
    }
    .left-actions, .right-actions {
      display: flex;
      gap: 6px;
      align-items: center;
    }
    .copy-wrap {
      position: relative;
      display: inline-flex;
      align-items: center;
      justify-content: center;
    }
    .copy-hover {
      position: absolute;
      right: 0;
      bottom: calc(100% + 6px);
      background: #2f7a54;
      border: 1px solid #4fa174;
      color: #ffffff;
      border-radius: 5px;
      padding: 3px 8px;
      font-size: 11px;
      line-height: 1;
      white-space: nowrap;
      opacity: 0;
      transform: translateY(4px);
      pointer-events: none;
      transition: opacity 140ms ease, transform 140ms ease;
    }
    .copy-hover.show {
      opacity: 1;
      transform: translateY(0);
    }
    .btn {
      min-width: 80px;
      height: 28px;
      border: 1px solid var(--btn-line);
      background: var(--btn-bg);
      color: #ffffff;
      border-radius: 5px;
      font-weight: 500;
      padding: 0 10px;
      cursor: pointer;
      font-size: 12px;
    }
    .btn:hover {
      background: #343842;
    }
    .status {
      margin-top: 4px;
      min-height: 16px;
      color: var(--muted);
      font-size: 11px;
    }
    @media (max-width: 900px) {
      .grid-header {
        display: none;
      }
      .row {
        grid-template-columns: 1fr;
        gap: 4px;
      }
      .actions {
        flex-direction: column;
        align-items: stretch;
      }
      .left-actions,
      .right-actions {
        width: 100%;
      }
      .btn {
        flex: 1;
      }
    }
  </style>
</head>
<body>
  <main class="wrap">
    <section class="frame">
      <section class="top-pane">
        <div class="grid-header">
          <div>項目名</div>
          <div>選択</div>
          <div>削除</div>
          <div>自由入力</div>
        </div>
        <div id="rows"></div>
      </section>
      <section class="bottom-pane">
        <div class="preview-title">Preview</div>
        <div id="preview" class="preview"></div>

        <div class="actions">
          <div class="left-actions">
            <button id="openHistory" class="btn">履歴を開く</button>
          </div>
          <div class="right-actions">
            <button id="reset" class="btn">Reset</button>
            <div class="copy-wrap">
              <button id="copy" class="btn">Copy</button>
              <div id="copyHover" class="copy-hover" role="status" aria-live="polite">コピーしました</div>
            </div>
          </div>
        </div>
        <div id="status" class="status"></div>
      </section>
    </section>
  </main>

  <script>
    const NO_SELECTION = "指定なし";
    const state = {
      rows: [],
      preview: "",
      confirm_delete: true,
    };
    let copyHoverTimer = null;

    function setStatus(message) {
      const status = document.getElementById("status");
      status.textContent = message || "";
    }

    function showCopyHover(message) {
      const hover = document.getElementById("copyHover");
      if (!hover) {
        return;
      }
      hover.textContent = message;
      hover.classList.add("show");
      if (copyHoverTimer) {
        clearTimeout(copyHoverTimer);
      }
      copyHoverTimer = setTimeout(() => {
        hover.classList.remove("show");
        copyHoverTimer = null;
      }, 1200);
    }

    async function apiGet(path) {
      const res = await fetch(path, { method: "GET" });
      const data = await res.json();
      if (!res.ok || !data.ok) {
        throw new Error(data.error || "request failed");
      }
      return data;
    }

    async function apiPost(path, body) {
      const res = await fetch(path, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body || {}),
      });
      const data = await res.json();
      if (!res.ok || !data.ok) {
        throw new Error(data.error || "request failed");
      }
      return data;
    }

    function applySnapshot(payload) {
      state.rows = payload.rows || [];
      state.preview = payload.preview || "";
      if (typeof payload.confirm_delete === "boolean") {
        state.confirm_delete = payload.confirm_delete;
      }
      render();
    }

    function render() {
      const rowsRoot = document.getElementById("rows");
      rowsRoot.innerHTML = "";

      for (const row of state.rows) {
        const wrapper = document.createElement("div");
        wrapper.className = "row";

        const label = document.createElement("div");
        label.className = "label";
        label.textContent = row.label;

        const select = document.createElement("select");
        for (const choice of row.choices) {
          const option = document.createElement("option");
          option.value = choice;
          option.textContent = choice;
          option.title = choice;
          if (choice === row.selected) {
            option.selected = true;
          }
          select.appendChild(option);
        }

        const del = document.createElement("button");
        del.className = "delete";
        del.textContent = "🗑";
        del.title = "選択中のキーワードを削除";
        del.disabled = !row.selected || row.selected === NO_SELECTION;

        const input = document.createElement("input");
        input.type = "text";
        input.placeholder = "Enterで確定";
        input.disabled = !row.allow_free_text;
        input.value = row.free_text || "";

        select.addEventListener("change", async () => {
          try {
            const data = await apiPost("/app/combo-change", {
              item_id: row.item_id,
              selected: select.value,
            });
            applySnapshot(data);
            setStatus("");
          } catch (err) {
            setStatus(`保存エラー: ${err.message}`);
          }
        });

        del.addEventListener("click", async () => {
          if (!select.value || select.value === NO_SELECTION) {
            return;
          }
          if (state.confirm_delete) {
            const ok = confirm(`${select.value}を一覧から削除しますか？`);
            if (!ok) {
              return;
            }
          }
          try {
            const data = await apiPost("/app/delete-choice", {
              item_id: row.item_id,
              selected: select.value,
            });
            applySnapshot(data);
            setStatus("");
          } catch (err) {
            setStatus(`削除エラー: ${err.message}`);
          }
        });

        input.addEventListener("keydown", async (event) => {
          if (event.key !== "Enter") {
            return;
          }
          event.preventDefault();
          try {
            const data = await apiPost("/app/free-confirm", {
              item_id: row.item_id,
              selected: select.value,
              value: input.value,
            });
            applySnapshot(data);
            setStatus("");
          } catch (err) {
            setStatus(`保存エラー: ${err.message}`);
          }
        });

        wrapper.appendChild(label);
        wrapper.appendChild(select);
        wrapper.appendChild(del);
        wrapper.appendChild(input);
        rowsRoot.appendChild(wrapper);
      }

      document.getElementById("preview").textContent = state.preview;
    }

    async function init() {
      try {
        const data = await apiGet("/app/init");
        applySnapshot(data);
      } catch (err) {
        setStatus(`起動エラー: ${err.message}`);
      }
    }

    document.getElementById("openHistory").addEventListener("click", async () => {
      try {
        await apiPost("/app/open-history", {});
        setStatus("");
      } catch (err) {
        setStatus(`履歴オープン失敗: ${err.message}`);
      }
    });

    document.getElementById("reset").addEventListener("click", async () => {
      const ok = confirm("選択内容をリセットしてもよろしいですか？");
      if (!ok) {
        return;
      }
      try {
        const data = await apiPost("/app/reset", {});
        applySnapshot(data);
        setStatus("");
      } catch (err) {
        setStatus(`リセット失敗: ${err.message}`);
      }
    });

    document.getElementById("copy").addEventListener("click", async () => {
      try {
        const prompt = state.preview || "";
        if (!prompt.trim()) {
          return;
        }
        const data = await apiPost("/app/copy", { prompt });
        if (data.skipped) {
          setStatus("連続コピーは間引かれました。");
        } else {
          setStatus("コピーしました。");
          showCopyHover("コピーしました");
        }
      } catch (err) {
        setStatus(`コピー失敗: ${err.message}`);
      }
    });

    init();
  </script>
</body>
</html>
"#;
