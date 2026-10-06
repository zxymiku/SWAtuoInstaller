<script lang="ts">
  /**
   * 日志档案页。
   *
   * 这是「显式细节层」：为扫描与分组优化，完整保留每条记录，
   * 支持按级别过滤、全文搜索与导出为带时间戳的文本文件
   * （导出由 Rust 侧 `export_logs` 写入 `{app_data_dir}/logs/`）。
   */
  import ActionButton from '$lib/components/ActionButton.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import SectionTitle from '$lib/components/SectionTitle.svelte';
  import StatusChip from '$lib/components/StatusChip.svelte';
  import { exportLogs as exportLogsCommand, logUiEvent } from '$lib/api/commands';
  import {
    clearLogs,
    installState,
    logs,
    popups,
    refreshStatus,
    shortTime,
    type LogEntry,
  } from '$lib/stores/install.svelte.ts';

  let exportNotice = $state<{ kind: 'ok' | 'error'; text: string } | null>(null);
  let exporting = $state(false);
  let lastRefresh = $state<string | null>(null);

  const errorCount = $derived(logs.filter((entry) => entry.level === 'error').length);
  const warnCount = $derived(logs.filter((entry) => entry.level === 'warn').length);
  const successCount = $derived(logs.filter((entry) => entry.level === 'success').length);

  /** 把日志行渲染成可导出的纯文本（含上下文表头）。 */
  function renderText(entries: LogEntry[]): string {
    const header = [
      '# SolidWorks 2024 SP5 自动安装程序 — 安装日志',
      `# 导出时间: ${new Date().toISOString()}`,
      `# 条目数: ${entries.length}`,
      `# 当前步骤: ${installState.currentStep}/9 · ${installState.message}`,
      `# 整体进度: ${installState.percent.toFixed(1)}%`,
      `# 已处理弹窗: ${installState.popupsHandled}`,
      '#',
      '# 时间戳                    级别     内容',
      '',
    ].join('\n');

    const body = entries
      .map(
        (entry) =>
          `${entry.timestamp}  ${entry.level.toUpperCase().padEnd(7)}  ${entry.message}`,
      )
      .join('\n');

    return `${header}${body}\n`;
  }

  async function exportLogs(entries: LogEntry[]): Promise<void> {
    void logUiEvent(`点击：导出日志；条目数=${entries.length}`);
    if (entries.length === 0) {
      void logUiEvent('响应：导出日志未执行，当前没有日志条目');
      return;
    }
    exporting = true;
    exportNotice = null;
    try {
      const path = await exportLogsCommand(renderText(entries), 'solidworks-install');
      exportNotice = { kind: 'ok', text: `已导出 ${entries.length} 条日志 → ${path}` };
      void logUiEvent(`响应：日志导出成功；${path}`);
    } catch (error) {
      exportNotice = {
        kind: 'error',
        text: `导出失败: ${error instanceof Error ? error.message : String(error)}`,
      };
      void logUiEvent(`响应：日志导出失败；${error instanceof Error ? error.message : String(error)}`);
    } finally {
      exporting = false;
    }
  }

  async function handleRefresh(): Promise<void> {
    void logUiEvent('点击：刷新安装状态');
    await refreshStatus();
    lastRefresh = new Date().toISOString();
    void logUiEvent('响应：安装状态刷新完成');
  }

  function handleClearLogs(): void {
    void logUiEvent('点击：清空日志（日志面板）');
    clearLogs();
    void logUiEvent('响应：日志缓冲已清空');
  }
</script>

<div class="logs-page">
  <SectionTitle
    index="LOG ARCHIVE / 03"
    title="日志档案"
    caption={`${logs.length} ENTRIES · ${popups.length} POPUPS`}
    note="保留本次会话的全部输出。过滤与搜索只影响展示，不丢弃数据；导出内容即当前过滤结果。"
    scale="page"
  />

  <div class="logs-page__toolbar">
    <div class="logs-page__readouts">
      <StatusChip label="总条目" value={String(logs.length)} />
      <StatusChip
        label="错误"
        value={String(errorCount)}
        state={errorCount > 0 ? 'error' : 'ok'}
      />
      <StatusChip
        label="警告"
        value={String(warnCount)}
        state={warnCount > 0 ? 'warn' : 'ok'}
      />
      <StatusChip label="成功" value={String(successCount)} state="ok" />
      <StatusChip
        label="部署状态"
        value={installState.running
          ? '进行中'
          : installState.finished
            ? installState.success
              ? '成功'
              : '失败'
            : '待命'}
        state={installState.running
          ? 'active'
          : installState.finished
            ? installState.success
              ? 'ok'
              : 'error'
            : 'idle'}
      />
    </div>

    <div class="logs-page__actions">
      <ActionButton
        label="刷新状态"
        size="compact"
        onclick={handleRefresh}
        title="从后端重新读取安装状态快照"
      />
      <ActionButton
        label={exporting ? '导出中…' : '导出全部'}
        size="compact"
        variant="primary"
        disabled={logs.length === 0 || exporting}
        onclick={() => exportLogs(logs)}
      />
      <ActionButton
        label="清空日志"
        size="compact"
        variant="danger"
        disabled={logs.length === 0}
        onclick={() => {
          void logUiEvent('点击：清空日志');
          clearLogs();
          exportNotice = { kind: 'ok', text: '已清空本次会话的日志缓冲' };
          void logUiEvent('响应：日志缓冲已清空');
        }}
      />
    </div>
  </div>

  {#if exportNotice}
    <p class="logs-page__notice" data-kind={exportNotice.kind} role="status">
      <span class="logs-page__notice-mark" aria-hidden="true">
        {exportNotice.kind === 'ok' ? '✓' : '×'}
      </span>
      <span>{exportNotice.text}</span>
    </p>
  {/if}

  {#if lastRefresh}
    <p class="logs-page__stamp">最近刷新 {shortTime(lastRefresh)}</p>
  {/if}

  <LogViewer
    entries={logs}
    full
    height="min(56svh, 620px)"
    onClear={handleClearLogs}
    onExport={exportLogs}
  />

  <section class="logs-page__popups" aria-labelledby="popup-title">
    <header class="logs-page__section-head">
      <div>
        <p class="logs-page__index">POPUP HANDLING / {popups.length}</p>
        <h3 id="popup-title" class="logs-page__title">弹窗处理记录</h3>
      </div>
      <p class="logs-page__note">
        守护线程每 <code>install.popup.scan_interval_ms</code> 毫秒扫描一次顶层窗口，
        命中后按配置执行动作并记录在这里。
      </p>
    </header>

    {#if popups.length === 0}
      <p class="logs-page__empty">本次会话尚未处理任何弹窗。</p>
    {:else}
      <div class="ark-scroll logs-page__popup-list">
        <table>
          <caption class="ark-visually-hidden">已处理的安装弹窗列表</caption>
          <thead>
            <tr>
              <th scope="col">时间</th>
              <th scope="col">弹窗标题</th>
              <th scope="col">执行动作</th>
            </tr>
          </thead>
          <tbody>
            {#each [...popups].reverse() as popup (popup.id)}
              <tr>
                <td class="logs-page__cell-time">{shortTime(popup.timestamp)}</td>
                <td>{popup.title}</td>
                <td>{popup.action}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </section>
</div>

<style>
  .logs-page {
    display: grid;
    gap: var(--ark-space-lg);
    min-width: 0;
  }

  .logs-page__toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--ark-space-md);
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-accent);
    background: var(--ark-paper);
  }

  .logs-page__readouts,
  .logs-page__actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ark-space-sm);
  }

  .logs-page__notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 0;
    padding: 10px 12px;
    border: var(--ark-rule) solid var(--ark-border);
    border-left-width: 3px;
    font-size: var(--ark-font-size-label);
    line-height: 1.6;
    word-break: break-all;
  }

  .logs-page__notice[data-kind='ok'] {
    border-left-color: var(--ark-state-ok);
    color: var(--ark-level-success);
    background: var(--ark-ok-wash);
  }

  .logs-page__notice[data-kind='error'] {
    border-left-color: var(--ark-state-error);
    color: var(--ark-state-error);
    background: var(--ark-error-wash);
  }

  .logs-page__notice-mark {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    background: currentColor;
    color: var(--ark-paper);
    font-family: var(--ark-font-mono);
    font-size: 12px;
    font-weight: 700;
  }

  .logs-page__stamp {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.1em;
    color: var(--ark-ink-mute);
    text-transform: uppercase;
  }

  .logs-page__popups {
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .logs-page__section-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--ark-space-md);
    flex-wrap: wrap;
  }

  .logs-page__index {
    margin: 0 0 4px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .logs-page__title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 20px;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: -0.01em;
  }

  .logs-page__note {
    margin: 0;
    max-width: 62ch;
    font-size: var(--ark-font-size-micro);
    line-height: 1.7;
    color: var(--ark-ink-mute);
  }

  .logs-page__note code {
    font-family: var(--ark-font-mono);
    padding: 1px 4px;
    background: var(--ark-paper-warm);
  }

  .logs-page__empty {
    margin: 0;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-label);
    color: var(--ark-ink-mute);
    text-align: center;
  }

  .logs-page__popup-list {
    max-height: 320px;
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
  }

  .logs-page__popup-list table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  .logs-page__popup-list th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 8px 10px;
    border-bottom: var(--ark-rule) solid var(--ark-ink);
    background: var(--ark-paper-warm);
    font-family: var(--ark-font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    text-align: left;
    color: var(--ark-ink-mute);
  }

  .logs-page__popup-list td {
    padding: 8px 10px;
    border-bottom: var(--ark-rule) solid var(--ark-border);
    vertical-align: top;
  }

  .logs-page__popup-list tr:last-child td {
    border-bottom: 0;
  }

  .logs-page__cell-time {
    font-family: var(--ark-font-mono);
    font-variant-numeric: tabular-nums;
    color: var(--ark-ink-mute);
    white-space: nowrap;
  }
</style>
