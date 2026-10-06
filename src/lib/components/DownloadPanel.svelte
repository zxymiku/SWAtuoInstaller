<script lang="ts">
  /**
   * 逐文件下载状态面板。
   *
   * 分片下载（例如 8 个网盘分卷）时，总进度条只能告诉你"整体到哪了"，
   * 看不出「哪一片卡住了」「哪一片名字被解析成了什么」。
   * 这个面板把每个文件单独列出来：文件名、进度、速度、阶段。
   *
   * 面板只在有分片/多文件下载数据时出现，单包下载不占版面。
   */
  import StatusChip from './StatusChip.svelte';
  import {
    type DownloadFileState,
    isFileActive,
  } from '$lib/stores/install.svelte.ts';

  interface Props {
    files: DownloadFileState[];
    /** 解压阶段的当前文件（有值时在下方显示一条） */
    extract?: { percent: number; currentFile: string; pass: number } | null;
  }

  let { files, extract = null }: Props = $props();

  const visible = $derived(files.length > 0);

  const doneCount = $derived(
    files.filter((f) => f.phase === 'done' || f.phase === 'skipped').length,
  );
  const failedCount = $derived(files.filter((f) => f.phase === 'failed').length);

  const overallDone = $derived(files.reduce((sum, f) => sum + f.bytesDone, 0));
  const overallTotal = $derived(files.reduce((sum, f) => sum + f.bytesTotal, 0));

  const overallSpeed = $derived(
    files.filter((f) => isFileActive(f.phase)).reduce((sum, f) => sum + f.speedBps, 0),
  );

  /** 阶段 → 展示文案与状态色。 */
  const PHASE_META: Record<string, { label: string; tone: string }> = {
    pending: { label: '排队', tone: 'idle' },
    resolving: { label: '解析地址', tone: 'busy' },
    downloading: { label: '下载中', tone: 'busy' },
    done: { label: '完成', tone: 'ok' },
    skipped: { label: '已存在', tone: 'ok' },
    failed: { label: '失败', tone: 'error' },
  };

  function phaseMeta(phase: string) {
    return PHASE_META[phase] ?? { label: phase, tone: 'idle' };
  }

  function percentOf(file: DownloadFileState): number {
    if (file.bytesTotal <= 0) return 0;
    return Math.min(100, (file.bytesDone / file.bytesTotal) * 100);
  }

  function sizeLabel(bytes: number): string {
    if (bytes <= 0) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${value.toFixed(value >= 100 || unit === 0 ? 0 : 1)} ${units[unit]}`;
  }

  function speedLabel(bps: number): string {
    if (bps <= 0) return '—';
    return `${sizeLabel(bps)}/s`;
  }

  /** 文件名太长时保留首尾，便于区分 `...001` / `...002`。 */
  function shortName(name: string): string {
    if (!name) return '（解析中…）';
    if (name.length <= 54) return name;
    return `${name.slice(0, 30)}…${name.slice(-20)}`;
  }
</script>

{#if visible}
  <section class="dl" aria-labelledby="dl-title">
    <header class="dl__head">
      <div>
        <p class="dl__index">TRANSFER MANIFEST</p>
        <h3 id="dl-title" class="dl__title">下载清单</h3>
      </div>
      <div class="dl__stats">
        <StatusChip
          label="文件"
          value={`${doneCount}/${files.length}`}
          state={failedCount > 0 ? 'error' : doneCount === files.length ? 'ok' : 'warn'}
        />
        {#if overallTotal > 0}
          <StatusChip
            label="已接收"
            value={`${sizeLabel(overallDone)} / ${sizeLabel(overallTotal)}`}
            state="idle"
          />
        {/if}
        {#if overallSpeed > 0}
          <StatusChip label="合计速度" value={speedLabel(overallSpeed)} state="idle" />
        {/if}
      </div>
    </header>

    <ol class="dl__list ark-scroll">
      {#each files as file (file.index)}
        {@const meta = phaseMeta(file.phase)}
        <li class="dl__row" data-tone={meta.tone}>
          <span class="dl__ordinal">{String(file.index).padStart(2, '0')}</span>

          <span class="dl__name" title={file.name}>
            {shortName(file.name)}
          </span>

          <span class="dl__meter" aria-hidden="true">
            <span class="dl__meter-fill" style={`width:${percentOf(file)}%`}></span>
          </span>

          <span class="dl__size">
            {sizeLabel(file.bytesDone)}{#if file.bytesTotal > 0}
              / {sizeLabel(file.bytesTotal)}{/if}
          </span>

          <span class="dl__speed">{speedLabel(file.speedBps)}</span>

          <span class="dl__phase" data-tone={meta.tone}>{meta.label}</span>

          {#if file.detail && file.phase === 'failed'}
            <span class="dl__detail" role="alert">{file.detail}</span>
          {/if}
        </li>
      {/each}
    </ol>

    {#if extract}
      <div class="dl__extract">
        <span class="dl__extract-label">
          解压（第 {extract.pass} 阶段）
        </span>
        <span class="dl__extract-file" title={extract.currentFile}>
          {shortName(extract.currentFile)}
        </span>
        <span class="dl__meter dl__meter--wide" aria-hidden="true">
          <span class="dl__meter-fill" style={`width:${extract.percent}%`}></span>
        </span>
        <span class="dl__size">{extract.percent.toFixed(0)}%</span>
      </div>
    {/if}
  </section>
{/if}

<style>
  .dl {
    display: grid;
    gap: 10px;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
    min-width: 0;
  }

  .dl__head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--ark-space-md);
    flex-wrap: wrap;
  }

  .dl__index {
    margin: 0 0 4px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .dl__title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 20px;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: -0.01em;
  }

  .dl__stats {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .dl__list {
    display: grid;
    gap: 2px;
    max-height: 340px;
  }

  .dl__row {
    display: grid;
    grid-template-columns: 26px minmax(0, 1.6fr) minmax(60px, 1fr) 96px 74px 62px;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-left: 3px solid transparent;
    background: var(--ark-paper-warm);
    font-size: var(--ark-font-size-micro);
    min-width: 0;
  }

  .dl__row[data-tone='busy'] {
    border-left-color: var(--ark-accent);
  }
  .dl__row[data-tone='ok'] {
    border-left-color: var(--ark-state-ok);
  }
  .dl__row[data-tone='error'] {
    border-left-color: var(--ark-state-error);
    background: var(--ark-error-wash);
  }

  .dl__ordinal {
    font-family: var(--ark-font-mono);
    font-size: 10px;
    color: var(--ark-ink-mute);
  }

  .dl__name {
    font-family: var(--ark-font-mono);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ark-ink);
  }

  .dl__meter {
    display: block;
    height: 4px;
    background: var(--ark-border);
    overflow: hidden;
  }

  .dl__meter--wide {
    flex: 1;
  }

  .dl__meter-fill {
    display: block;
    height: 100%;
    background: var(--ark-accent);
    transition: width 180ms linear;
  }

  .dl__row[data-tone='ok'] .dl__meter-fill {
    background: var(--ark-state-ok);
  }
  .dl__row[data-tone='error'] .dl__meter-fill {
    background: var(--ark-state-error);
  }

  .dl__size,
  .dl__speed {
    font-family: var(--ark-font-mono);
    font-size: 10px;
    color: var(--ark-ink-soft);
    text-align: right;
    white-space: nowrap;
  }

  .dl__phase {
    padding: 1px 6px;
    font-family: var(--ark-font-mono);
    font-size: 10px;
    text-align: center;
    letter-spacing: 0.04em;
    background: var(--ark-paper);
    color: var(--ark-ink-mute);
  }

  .dl__phase[data-tone='busy'] {
    color: var(--ark-accent);
  }
  .dl__phase[data-tone='ok'] {
    color: var(--ark-level-success);
  }
  .dl__phase[data-tone='error'] {
    color: var(--ark-level-error);
    font-weight: 700;
  }

  .dl__detail {
    grid-column: 2 / -1;
    font-size: var(--ark-font-size-micro);
    line-height: 1.6;
    color: var(--ark-level-error);
    word-break: break-all;
  }

  .dl__extract {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 8px;
    border-top: var(--ark-rule) solid var(--ark-border);
    font-size: var(--ark-font-size-micro);
    min-width: 0;
  }

  .dl__extract-label {
    font-family: var(--ark-font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
    white-space: nowrap;
  }

  .dl__extract-file {
    flex: 0 1 auto;
    max-width: 42%;
    font-family: var(--ark-font-mono);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ark-ink-soft);
  }
</style>
