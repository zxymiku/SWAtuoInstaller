<script lang="ts">
  /**
   * 环境实测面板：把 Rust 侧 `env_check` 的真实读数摊成器械读数表。
   *
   * 这里只展示后端真正读取到的值（GetUserNameW / GetComputerNameExW /
   * GetTokenInformation / GetDiskFreeSpaceExW），不制造装饰性遥测。
   */
  import StatusChip from './StatusChip.svelte';
  import type { EnvSnapshot } from '$lib/api/types';
  import { environmentWarning } from '$lib/stores/config.svelte.ts';

  interface Props {
    snapshot: EnvSnapshot | null;
    loading: boolean;
    error: string | null;
    onRefresh: () => void;
  }

  let { snapshot, loading, error, onRefresh }: Props = $props();

  const warning = $derived(environmentWarning(snapshot));

  function asciiState(value: boolean | undefined): 'ok' | 'error' | 'idle' {
    if (value === undefined) return 'idle';
    return value ? 'ok' : 'error';
  }

  function spaceState(gb: number | undefined): 'ok' | 'warn' | 'idle' {
    if (gb === undefined || gb < 0) return 'idle';
    return gb >= 20 ? 'ok' : 'warn';
  }
</script>

<section class="env" aria-labelledby="env-title">
  <header class="env__head">
    <div>
      <p class="env__index">ENVIRONMENT PROBE</p>
      <h3 id="env-title" class="env__title">运行环境实测</h3>
    </div>
    <button
      type="button"
      class="ark-btn ark-btn--compact"
      onclick={onRefresh}
      disabled={loading}
    >
      <span class="ark-btn__label">{loading ? '读取中…' : '重新检测'}</span>
    </button>
  </header>

  {#if error}
    <p class="env__error" role="alert">{error}</p>
  {/if}

  {#if warning}
    <p class="env__warning" role="alert">
      <span class="env__warning-mark" aria-hidden="true">!</span>
      {warning}
    </p>
  {/if}

  <dl class="env__grid">
    <div class="env__cell">
      <dt>用户名 / USER</dt>
      <dd>{snapshot?.username ?? '—'}</dd>
      <StatusChip
        label="ASCII"
        value={snapshot ? (snapshot.username_ascii ? '通过' : '含非 ASCII') : '未读取'}
        state={asciiState(snapshot?.username_ascii)}
      />
    </div>

    <div class="env__cell">
      <dt>计算机名 / HOST</dt>
      <dd>{snapshot?.computer_name ?? '—'}</dd>
      <StatusChip
        label="ASCII"
        value={snapshot ? (snapshot.computer_ascii ? '通过' : '含非 ASCII') : '未读取'}
        state={asciiState(snapshot?.computer_ascii)}
      />
    </div>

    <div class="env__cell">
      <dt>管理员权限 / ELEVATION</dt>
      <dd>{snapshot ? (snapshot.elevated ? '已提升' : '标准用户') : '—'}</dd>
      <StatusChip
        label="UAC"
        value={snapshot ? (snapshot.elevated ? '已通过' : '需提权') : '未读取'}
        state={snapshot?.elevated ? 'ok' : 'warn'}
      />
    </div>

    <div class="env__cell">
      <dt>目标盘可用空间</dt>
      <dd>
        {snapshot && snapshot.free_space_gb >= 0
          ? `${snapshot.free_space_gb.toFixed(1)} GB`
          : '未读取'}
      </dd>
      <StatusChip
        label="需求"
        value="≥ 20 GB"
        state={spaceState(snapshot?.free_space_gb)}
      />
    </div>

    <div class="env__cell env__cell--wide">
      <dt>解压程序 / 7Z</dt>
      <dd class="env__path">{snapshot?.seven_zip.program ?? '未检测'}</dd>
      <StatusChip
        label="来源"
        value={snapshot?.seven_zip.source ?? '—'}
        state={snapshot ? (snapshot.seven_zip.available ? 'ok' : 'error') : 'idle'}
      />
      {#if snapshot && !snapshot.seven_zip.available}
        <p class="env__inline-hint">
          未找到 7z。开始部署时会{snapshot.seven_zip.auto_download
            ? '自动从 download_url 获取并缓存到应用数据目录'
            : '直接报错（auto_download 已关闭）'}。
        </p>
      {/if}
    </div>

    <div class="env__cell env__cell--wide">
      <dt>配置文件 / CONFIG</dt>
      <dd class="env__path">{snapshot?.config_path ?? '—'}</dd>
      <StatusChip
        label="状态"
        value={snapshot ? (snapshot.config_exists ? '已存在' : '将创建') : '未读取'}
        state="idle"
      />
    </div>

    <div class="env__cell env__cell--wide">
      <dt>应用数据目录 / APP DATA</dt>
      <dd class="env__path">{snapshot?.app_data_dir ?? '—'}</dd>
    </div>

    <div class="env__cell env__cell--wide">
      <dt>临时目录 / TEMP</dt>
      <dd class="env__path">{snapshot?.temp_dir ?? '—'}</dd>
    </div>
  </dl>
</section>

<style>
  .env {
    display: grid;
    gap: var(--ark-space-sm);
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-ink);
    background: var(--ark-paper);
  }

  .env__head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--ark-space-md);
  }

  .env__index {
    margin: 0 0 4px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    color: var(--ark-ink-mute);
  }

  .env__title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 20px;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: -0.01em;
  }

  .env__error {
    margin: 0;
    padding: 8px 10px;
    border-left: 3px solid var(--ark-state-error);
    background: var(--ark-error-wash);
    font-size: var(--ark-font-size-label);
    color: var(--ark-state-error);
  }

  .env__warning {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0;
    padding: 9px 10px;
    border-left: 3px solid var(--ark-state-warn);
    background: var(--ark-warn-wash);
    font-size: var(--ark-font-size-label);
    line-height: 1.6;
    color: var(--ark-level-warn);
  }

  .env__warning-mark {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    flex: 0 0 auto;
    margin-top: 1px;
    background: var(--ark-state-warn);
    color: var(--ark-paper);
    font-family: var(--ark-font-mono);
    font-size: 11px;
    font-weight: 700;
  }

  .env__grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 1px;
    background: var(--ark-border);
    border: var(--ark-rule) solid var(--ark-border);
  }

  .env__cell {
    display: grid;
    align-content: start;
    gap: 6px;
    padding: 10px 12px;
    background: var(--ark-paper);
    min-width: 0;
  }

  .env__cell--wide {
    grid-column: 1 / -1;
  }

  .env__cell dt {
    font-family: var(--ark-font-mono);
    font-size: 9px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .env__cell dd {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    word-break: break-all;
  }

  .env__path {
    font-family: var(--ark-font-mono);
    font-size: 12px;
    font-weight: 500;
    line-height: 1.5;
    color: var(--ark-ink-soft);
  }

  .env__cell :global(.ark-chip) {
    justify-self: start;
  }

  .env__inline-hint {
    margin: 2px 0 0;
    font-size: var(--ark-font-size-micro);
    line-height: 1.6;
    color: var(--ark-level-warn);
  }
</style>
