<script lang="ts">
  /**
   * 进度与预估：状态驱动仪表。
   *
   * 显示通道随真实状态切换：
   *   下载 → 字节读数 + 速度 + 分块数
   *   解压 → 当前文件名 + 阶段
   *   安装 → 阶段说明 + 检测项进度
   */
  import {
    formatBytes,
    formatEta,
    formatPercent,
    formatSpeed,
    type InstallState,
  } from '$lib/stores/install.svelte.ts';

  interface Props {
    state: InstallState;
    /** 已运行时长文本（由页面每秒刷新） */
    elapsed: string;
  }

  let { state, elapsed }: Props = $props();

  const channelLabel = $derived(
    state.activeChannel === 'download'
      ? 'DOWNLOAD'
      : state.activeChannel === 'extract'
        ? 'EXTRACT'
        : state.activeChannel === 'install'
          ? 'INSTALL'
          : 'STANDBY',
  );

  const emphasisValue = $derived(
    state.activeChannel === 'download' && state.download.bytesTotal > 0
      ? formatBytes(state.download.bytesDone)
      : `${formatPercent(state.percent)}%`,
  );

  const emphasisUnit = $derived(
    state.activeChannel === 'download' && state.download.bytesTotal > 0
      ? ` / ${formatBytes(state.download.bytesTotal)}`
      : '%',
  );
</script>

<section class="progress" aria-label="安装进度">
  <div class="progress__head">
    <div>
      <p class="progress__kicker">
        <span>CHANNEL · {channelLabel}</span>
      </p>
      <p class="progress__value">
        {emphasisValue}<span class="progress__unit">{emphasisUnit}</span>
      </p>
    </div>

    <dl class="progress__metrics">
      <div>
        <dt>已用时间</dt>
        <dd>{elapsed}</dd>
      </div>
      <div>
        <dt>预计剩余</dt>
        <dd>{formatEta(state.etaSeconds)}</dd>
      </div>
      <div>
        <dt>当前步骤</dt>
        <dd>{state.currentStep || '—'} / {state.totalSteps}</dd>
      </div>
      <div>
        <dt>弹窗处理</dt>
        <dd>{state.popupsHandled}</dd>
      </div>
    </dl>
  </div>

  <div
    class="progress__track"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={Math.round(state.percent)}
    aria-label="整体进度"
  >
    <div class="progress__fill" style="width: {Math.min(100, Math.max(0, state.percent))}%"></div>
  </div>
  <div class="progress__ticks" aria-hidden="true"></div>

  <div class="progress__meta">
    <span>{state.message}</span>
    {#if state.activeChannel === 'download' && state.download.chunksTotal > 0}
      <span>
        分块 {state.download.chunksDone} / {state.download.chunksTotal}
      </span>
      <span>速度 {formatSpeed(state.download.speedBps)}</span>
    {:else if state.activeChannel === 'extract' && state.extract.currentFile}
      <span>阶段 {state.extract.pass} · {state.extract.currentFile}</span>
    {/if}
    {#if state.networkDisabled}
      <span class="progress__flag">网络已隔离</span>
    {/if}
    {#if state.isoMounted}
      <span class="progress__flag">镜像已挂载</span>
    {/if}
  </div>
</section>

<style>
  .progress {
    display: grid;
    gap: 10px;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-accent);
    background: var(--ark-paper);
  }

  .progress__head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--ark-space-md);
  }

  .progress__kicker {
    margin: 0 0 6px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .progress__value {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: clamp(38px, 5vw, 56px);
    font-weight: 800;
    line-height: 0.86;
    letter-spacing: -0.04em;
    font-variant-numeric: tabular-nums;
  }

  .progress__unit {
    margin-left: 6px;
    font-family: var(--ark-font-mono);
    font-size: 14px;
    font-weight: 500;
    letter-spacing: 0.04em;
    color: var(--ark-ink-mute);
  }

  .progress__metrics {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(96px, 1fr));
    gap: var(--ark-space-sm) var(--ark-space-md);
    min-width: min(100%, 380px);
  }

  .progress__metrics > div {
    display: grid;
    gap: 2px;
    padding-left: 10px;
    border-left: var(--ark-rule) solid var(--ark-border);
  }

  .progress__metrics dt {
    font-family: var(--ark-font-mono);
    font-size: 9px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .progress__metrics dd {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: 15px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .progress__track {
    position: relative;
    height: 16px;
    border: var(--ark-rule) solid var(--ark-ink);
    background: var(--ark-paper-warm);
    overflow: hidden;
  }

  .progress__fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: var(--ark-accent);
    transition: width var(--ark-motion-reveal) var(--ark-ease);
  }

  .progress__fill::after {
    content: '';
    position: absolute;
    inset: 0 0 0 auto;
    width: 4px;
    background: var(--ark-ink);
  }

  .progress__ticks {
    height: 10px;
    margin-top: -8px;
    opacity: var(--ark-track-opacity);
    background-image: repeating-linear-gradient(
      90deg,
      var(--ark-line-guide) 0,
      var(--ark-line-guide) 1px,
      transparent 1px,
      transparent 5%
    );
  }

  .progress__meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ark-space-md);
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.08em;
    color: var(--ark-ink-mute);
    text-transform: uppercase;
  }

  .progress__flag {
    padding: 0 6px;
    background: var(--ark-accent-wash);
    color: var(--ark-ink);
    font-weight: 700;
  }
</style>
