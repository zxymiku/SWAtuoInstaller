<script lang="ts">
  /**
   * 日志查看器。
   *
   * 三种用途共用一套实现：
   *   · 主页 —— 紧凑模式，只看最近若干行，自动滚到底
   *   · 日志页 —— 完整模式，按级别过滤 / 搜索 / 导出
   *
   * 过滤与搜索只影响展示，不丢弃数据；导出始终导出当前过滤结果，
   * 并在按钮上写明导出条目数，避免"看不见的内容被静默导出"。
   */
  import { shortTime, type LogEntry } from '$lib/stores/install.svelte.ts';
  import type { LogLevel } from '$lib/api/types';

  interface Props {
    entries: LogEntry[];
    /** 完整模式显示工具条 */
    full?: boolean;
    /** 紧凑模式最多显示多少行 */
    limit?: number;
    /** 容器高度 */
    height?: string;
    onDock?: boolean;
    /** 清空回调（仅完整模式） */
    onClear?: () => void;
    /** 导出回调（仅完整模式） */
    onExport?: (entries: LogEntry[]) => void;
  }

  let {
    entries,
    full = false,
    limit = 20,
    height = '260px',
    onDock = false,
    onClear,
    onExport,
  }: Props = $props();

  const LEVELS: { id: LogLevel | 'all'; label: string }[] = [
    { id: 'all', label: '全部' },
    { id: 'info', label: '信息' },
    { id: 'success', label: '成功' },
    { id: 'warn', label: '警告' },
    { id: 'error', label: '错误' },
    { id: 'debug', label: '调试' },
  ];

  let levelFilter = $state<LogLevel | 'all'>('all');
  let query = $state('');
  let autoscroll = $state(true);
  let viewport = $state<HTMLDivElement | null>(null);

  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return entries.filter((entry) => {
      if (levelFilter !== 'all' && entry.level !== levelFilter) return false;
      if (needle && !entry.message.toLowerCase().includes(needle)) return false;
      return true;
    });
  });

  const visible = $derived(full ? filtered : filtered.slice(-limit));

  const counts = $derived.by(() => {
    const result: Record<string, number> = { all: entries.length };
    for (const entry of entries) {
      result[entry.level] = (result[entry.level] ?? 0) + 1;
    }
    return result;
  });

  /** 新日志到达时保持视口在底部（用户往上翻阅时不打扰）。 */
  $effect(() => {
    if (!autoscroll || !viewport) return;
    // 依赖 entries.length 以在新消息到达时触发。
    void entries.length;
    viewport.scrollTop = viewport.scrollHeight;
  });
</script>

<section class="logs" aria-label="安装日志">
  {#if full}
    <div class="logs__toolbar">
      <div class="logs__filters" role="group" aria-label="按级别过滤">
        {#each LEVELS as level (level.id)}
          <button
            type="button"
            class="logs__filter"
            aria-pressed={levelFilter === level.id}
            onclick={() => {
              levelFilter = level.id;
            }}
          >
            {level.label}
            <span class="logs__count">{counts[level.id] ?? 0}</span>
          </button>
        {/each}
      </div>

      <div class="logs__tools">
        <label class="logs__search">
          <span class="ark-visually-hidden">搜索日志内容</span>
          <input
            class="ark-input"
            type="search"
            placeholder="搜索日志内容…"
            bind:value={query}
          />
        </label>

        <label class="ark-switch logs__autoscroll">
          <input type="checkbox" bind:checked={autoscroll} />
          <span class="ark-switch__track"></span>
          <span class="ark-switch__text">自动滚动</span>
        </label>

        <button
          type="button"
          class="ark-btn ark-btn--compact"
          disabled={filtered.length === 0 || !onExport}
          onclick={() => onExport?.(filtered)}
        >
          <span class="ark-btn__label">导出 {filtered.length} 条</span>
        </button>

        <button
          type="button"
          class="ark-btn ark-btn--compact ark-btn--danger"
          disabled={entries.length === 0 || !onClear}
          onclick={() => onClear?.()}
        >
          <span class="ark-btn__label">清空</span>
        </button>
      </div>
    </div>
  {:else}
    <div class="logs__compact-bar">
      <p class="logs__compact-meta">
        显示最近 {Math.min(limit, visible.length)} / {entries.length} 条
      </p>
      <label class="ark-switch logs__autoscroll">
        <input type="checkbox" bind:checked={autoscroll} />
        <span class="ark-switch__track"></span>
        <span class="ark-switch__text">自动滚动</span>
      </label>
    </div>
  {/if}

  <div
    class="ark-scroll logs__viewport"
    class:ark-scroll--fade={!full}
    style="height: {height}"
    bind:this={viewport}
    onscroll={() => {
      if (!full || !viewport) return;
      const atBottom =
        viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 24;
      autoscroll = atBottom;
    }}
  >
    {#if visible.length === 0}
      <p class="logs__empty">
        {entries.length === 0 ? '暂无日志输出' : '当前过滤条件下没有匹配的日志'}
      </p>
    {:else}
      <div class="ark-log" class:ark-log--dock={onDock}>
        {#each visible as entry (entry.id)}
          <div class="ark-log__row" data-level={entry.level}>
            <span class="ark-log__time">{shortTime(entry.timestamp)}</span>
            <span class="ark-log__level">{entry.level}</span>
            <span class="ark-log__message">{entry.message}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  {#if !full}
    <p class="logs__foot">
      日志缓冲上限 2000 条，超出后自动丢弃最早的记录 · 完整记录见「日志档案」页
    </p>
  {/if}
</section>

<style>
  .logs {
    display: grid;
    gap: var(--ark-space-sm);
    min-width: 0;
  }

  .logs__compact-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--ark-space-sm);
  }

  .logs__compact-meta {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.08em;
    color: var(--ark-ink-mute);
    text-transform: uppercase;
  }

  .logs__toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--ark-space-sm);
  }

  .logs__filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .logs__filter {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 10px;
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
    font-size: var(--ark-font-size-label);
    letter-spacing: 0.06em;
    cursor: pointer;
    transition:
      background var(--ark-motion-direct) var(--ark-ease),
      color var(--ark-motion-direct) var(--ark-ease);
  }

  .logs__filter[aria-pressed='true'] {
    background: var(--ark-ink);
    border-color: var(--ark-ink);
    color: var(--ark-paper);
  }

  .logs__count {
    font-family: var(--ark-font-mono);
    font-size: 10px;
    color: var(--ark-ink-mute);
  }

  .logs__filter[aria-pressed='true'] .logs__count {
    color: var(--ark-accent);
  }

  .logs__tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ark-space-sm);
  }

  .logs__search {
    display: block;
    min-width: min(260px, 40vw);
  }

  .logs__search .ark-input {
    min-height: 36px;
  }

  .logs__autoscroll {
    min-height: 36px;
  }

  .logs__viewport {
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
    padding: var(--ark-space-xs) 0;
    min-width: 0;
  }

  .logs__empty {
    padding: var(--ark-space-md);
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-label);
    color: var(--ark-ink-mute);
    text-align: center;
  }

  .logs__foot {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.08em;
    color: var(--ark-ink-mute);
    text-transform: uppercase;
  }
</style>
