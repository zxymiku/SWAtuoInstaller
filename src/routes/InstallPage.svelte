<script lang="ts">
  /**
   * 主页（安装向导）。
   *
   * 逐屏编排（maximal）：
   *   页头导语 → 9 步追踪器 + 状态仪表 → 行动条 → 实时日志
   * 所有读数都来自后端，装饰层只承担坐标、方向与区块标识。
   */
  import ActionButton from '$lib/components/ActionButton.svelte';
  import AntivirusPanel from '$lib/components/AntivirusPanel.svelte';
  import DownloadPanel from '$lib/components/DownloadPanel.svelte';
  import EnvironmentPanel from '$lib/components/EnvironmentPanel.svelte';
  import InstallProgress from '$lib/components/InstallProgress.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import SectionTitle from '$lib/components/SectionTitle.svelte';
  import StatusChip from '$lib/components/StatusChip.svelte';
  import StepTracker from '$lib/components/StepTracker.svelte';
  import {
    chooseLocalArchive,
    chooseLocalArchiveFolder,
    clearLocalArchive,
    config,
    configRuntime,
    environments,
    loadEnvironment,
    loadConfig,
  } from '$lib/stores/config.svelte.ts';
  import {
    STEPS,
    elapsedLabel,
    installState,
    logs,
    popups,
    recheckAntivirus,
    acknowledgeAntivirusResolved,
    requestCancel,
    startInstall,
  } from '$lib/stores/install.svelte.ts';
  import { logUiEvent } from '$lib/api/commands';

  interface Props {
    onNavigate: (id: string) => void;
  }

  let { onNavigate }: Props = $props();

  /** 每秒推进一次「已用时间」读数。 */
  let now = $state(Date.now());
  $effect(() => {
    const timer = window.setInterval(() => {
      now = Date.now();
    }, 1000);
    return () => window.clearInterval(timer);
  });

  const elapsed = $derived(elapsedLabel(installState.startedAt, now));

  const targetDirectory = $derived(
    config.install.install_path.trim() !== ''
      ? config.install.install_path.trim()
      : `${config.install.install_drive.trim() || 'C'}:\\SW`,
  );

  const sourceLabel = $derived(
    configRuntime.localArchive
      ? '本地压缩包'
      : config.download.url.trim() !== ''
        ? '分块下载'
        : '未配置',
  );

  const primaryLabel = $derived(
    installState.running
      ? '部署进行中…'
      : installState.finished
        ? installState.success
          ? '重新部署'
          : '重试部署'
        : '开始自动部署',
  );

  const canStart = $derived(
    !installState.running && (config.download.url.trim() !== '' || configRuntime.localArchive !== null),
  );

  let startError = $state<string | null>(null);

  async function handleStart(): Promise<void> {
    void logUiEvent('点击：开始自动部署');
    startError = null;
    const error = await startInstall($state.snapshot(config));
    if (error) startError = error;
  }

  const envAlert = $derived(
    installState.environmentAlert ??
      (environments.snapshot
        ? !environments.snapshot.computer_ascii
          ? `计算机名「${environments.snapshot.computer_name}」含非 ASCII 字符，FlexNet 许可服务将无法启动。`
          : !environments.snapshot.username_ascii
            ? `用户名「${environments.snapshot.username}」含非 ASCII 字符，建议改用英文账户。`
            : !environments.snapshot.elevated
              ? '当前未以管理员身份运行，注册表导入与服务安装会失败。'
              : null
        : null),
  );
</script>

<div class="install">
  <SectionTitle
    index="INSTALL WIZARD / 01"
    title="自动部署"
    caption="SOLIDWORKS 2024 SP5 · 13 STAGES / 9 STEPS"
    note="全流程由 config.toml 驱动：环境检测 → 获取压缩包 → 网络隔离 → 两阶段解压 → 注册表 → 挂载镜像 → 静默安装 → 轮询检测 → 收尾恢复。"
    scale="page"
  />

  {#if envAlert}
    <p class="install__alert" role="alert">
      <span class="install__alert-mark" aria-hidden="true">!</span>
      <span>{envAlert}</span>
    </p>
  {/if}

  {#if startError}
    <p class="install__alert install__alert--error" role="alert">
      <span class="install__alert-mark" aria-hidden="true">×</span>
      <span>{startError}</span>
    </p>
  {/if}

  <div class="install__grid">
    <section class="install__column" aria-label="步骤追踪">
      <div class="install__column-head">
        <p class="install__column-index">STEP TRACKER / 01–09</p>
        <p class="install__column-note">
          当前步骤高亮为信号黄；已完成步骤填充炭黑编号块。
        </p>
      </div>
      <StepTracker
        steps={STEPS}
        currentStep={installState.currentStep}
        completedSteps={installState.completedSteps}
        running={installState.running}
        onSelect={() => onNavigate('logs')}
      />
    </section>

    <section class="install__column" aria-label="状态仪表">
      <div class="install__column-head">
        <p class="install__column-index">STATUS INSTRUMENT</p>
        <p class="install__column-note">进度、剩余时间与实时状态读数。</p>
      </div>

      <InstallProgress state={installState} {elapsed} />

      <div class="install__facts">
        <StatusChip label="来源" value={sourceLabel} state={sourceLabel === '未配置' ? 'warn' : 'ok'} />
        <StatusChip label="线程" value={String(config.download.threads)} />
        <StatusChip
          label="目标"
          value={config.install.install_drive || 'C'}
          title={targetDirectory}
        />
        <StatusChip
          label="轮询超时"
          value={`${config.install.polling.timeout_minutes} 分钟`}
        />
        <StatusChip
          label="网络隔离"
          value={config.network.disable_network ? '启用' : '关闭'}
          state={config.network.disable_network ? 'active' : 'idle'}
        />
        <StatusChip
          label="弹窗处理"
          value={
            config.install.popup.auto_click_confirm || config.install.popup.auto_click_yes
              ? '自动'
              : '手动'
          }
          state={
            config.install.popup.auto_click_confirm || config.install.popup.auto_click_yes
              ? 'ok'
              : 'warn'
          }
        />
      </div>

      {#if popups.length > 0}
        <div class="install__popups">
          <p class="install__column-index">POPUP LOG / 已自动处理 {popups.length} 个弹窗</p>
          <ul>
            {#each popups.slice(-4).reverse() as popup (popup.id)}
              <li>
                <span class="install__popup-title">{popup.title}</span>
                <span class="install__popup-action">{popup.action}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </section>
  </div>

  <section class="install__actions" aria-label="部署操作">
    <div class="install__actions-main">
      <ActionButton
        label={primaryLabel}
        variant="primary"
        disabled={!canStart}
        onclick={handleStart}
      />
      <ActionButton
        label={installState.running ? '取消部署' : '取消（待命）'}
        variant="danger"
        disabled={!installState.running}
        onclick={requestCancel}
      />
      <ActionButton
        label={configRuntime.selectingArchive ? '选择中…' : configRuntime.localArchive ? '更换本地压缩包' : '选择本地压缩包'}
        disabled={installState.running || configRuntime.selectingArchive}
        onclick={chooseLocalArchive}
      />
      <ActionButton
        label={configRuntime.selectingArchive ? '选择中…' : '选择分卷文件夹'}
        disabled={installState.running || configRuntime.selectingArchive}
        onclick={chooseLocalArchiveFolder}
      />
      {#if configRuntime.localArchive}
        <ActionButton
          label="切回下载模式"
          variant="dock"
          disabled={installState.running || configRuntime.selectingArchive}
          onclick={clearLocalArchive}
        />
      {/if}
      <ActionButton
        label="打开设置"
        disabled={installState.running || configRuntime.selectingArchive}
        onclick={() => {
          void logUiEvent('点击：打开设置');
          onNavigate('settings');
        }}
      />
    </div>

    <dl class="install__readout">
      <div>
        <dt>安装目标</dt>
        <dd title={targetDirectory}>{targetDirectory}</dd>
      </div>
      <div>
        <dt>压缩包来源</dt>
        <dd title={configRuntime.localArchive ?? config.download.url}>
          {configRuntime.localArchive ?? (config.download.url || '未配置')}
        </dd>
      </div>
      <div>
        <dt>开始时间</dt>
        <dd>{installState.startedAt ? installState.startedAt.slice(0, 19) : '—'}</dd>
      </div>
      <div>
        <dt>结束时间</dt>
        <dd>{installState.finishedAt ? installState.finishedAt.slice(0, 19) : '—'}</dd>
      </div>
    </dl>
  </section>

  <DownloadPanel
    files={installState.downloadFiles}
    extract={installState.extract}
  />

  <AntivirusPanel
    report={installState.antivirus}
    running={installState.running}
    onRecheck={() => {
      void recheckAntivirus();
    }}
    onAcknowledgeResolved={acknowledgeAntivirusResolved}
  />

  <div class="install__meta-grid">
    <EnvironmentPanel
      snapshot={environments.snapshot}
      loading={environments.loading}
      error={environments.error}
      onRefresh={() => {
        void loadEnvironment();
        void loadConfig();
      }}
    />

    <section class="install__meta" aria-labelledby="install-meta-title">
      <p class="install__column-index">FLOW NOTES</p>
      <h3 id="install-meta-title" class="install__meta-title">执行须知</h3>
      <ul class="install__meta-list">
        <li>
          <strong>管理员权限</strong>：注册表导入、网卡禁用、服务安装都要求提升权限，缺失时流程会在第 1 步终止。
        </li>
        <li>
          <strong>网络隔离</strong>：仅禁用非虚拟物理网卡；Drop 守卫保证异常退出后也会恢复。
        </li>
        <li>
          <strong>7z 解压</strong>：解压程序按「配置 → 内置资源 → 资源目录 → PATH」四级查找，全部失败会明确报错。
        </li>
        <li>
          <strong>完成判定</strong>：日志、进程、可执行文件三项中至少满足两项；超时阈值来自
          <code>install.polling.timeout_minutes</code>。
        </li>
        <li>
          <strong>弹窗守护</strong>：{"「重启」「不能核实此服务器存在」「端口@服务器」"}三类对话框会自动处理，策略可在设置页调整。
        </li>
        <li>
          <strong>失败排查</strong>：把 <code>workdir.cleanup_temp_after</code> 设为 false 可保留解压产物与日志。
        </li>
      </ul>
    </section>
  </div>

  <section class="install__logs" aria-labelledby="install-logs-title">
    <header class="install__logs-head">
      <div>
        <p class="install__column-index">LIVE LOG / 最近 20 条</p>
        <h3 id="install-logs-title" class="install__meta-title">实时输出</h3>
      </div>
      <ActionButton label="日志档案" size="compact" onclick={() => onNavigate('logs')} />
    </header>
    <LogViewer entries={logs} height="260px" limit={20} />
  </section>
</div>

<style>
  .install {
    display: grid;
    gap: var(--ark-space-lg);
    min-width: 0;
  }

  .install__alert {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 0;
    padding: 10px 12px;
    border: var(--ark-rule) solid var(--ark-state-warn);
    border-left-width: 3px;
    background: var(--ark-warn-wash);
    font-size: var(--ark-font-size-label);
    line-height: 1.65;
    color: var(--ark-level-warn);
  }

  .install__alert--error {
    border-color: var(--ark-state-error);
    background: var(--ark-error-wash);
    color: var(--ark-state-error);
  }

  .install__alert-mark {
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

  .install__grid {
    display: grid;
    grid-template-columns: minmax(0, 1.05fr) minmax(0, 1fr);
    gap: var(--ark-space-lg);
    align-items: start;
    min-width: 0;
  }

  .install__column {
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .install__column-head {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .install__column-index {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .install__column-note {
    margin: 0;
    font-size: var(--ark-font-size-micro);
    line-height: 1.6;
    color: var(--ark-ink-mute);
  }

  .install__facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .install__popups {
    display: grid;
    gap: 6px;
    padding: 10px var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-accent);
    background: var(--ark-paper);
  }

  .install__popups ul {
    display: grid;
    gap: 4px;
  }

  .install__popups li {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--ark-space-md);
    padding-bottom: 4px;
    border-bottom: var(--ark-rule) solid var(--ark-border);
    font-size: 12px;
  }

  .install__popups li:last-child {
    border-bottom: 0;
    padding-bottom: 0;
  }

  .install__popup-title {
    font-weight: 700;
    word-break: break-word;
  }

  .install__popup-action {
    font-family: var(--ark-font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--ark-ink-mute);
    white-space: nowrap;
  }

  .install__actions {
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(0, 1fr);
    align-items: center;
    gap: var(--ark-space-md);
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-ink);
    background: var(--ark-paper);
  }

  .install__actions-main {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ark-space-sm);
  }

  .install__readout {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--ark-space-sm) var(--ark-space-md);
    min-width: 0;
  }

  .install__readout > div {
    display: grid;
    gap: 2px;
    padding-left: 10px;
    border-left: var(--ark-rule) solid var(--ark-border);
    min-width: 0;
  }

  .install__readout dt {
    font-family: var(--ark-font-mono);
    font-size: 9px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .install__readout dd {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: 12px;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .install__meta-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--ark-space-lg);
    align-items: start;
    min-width: 0;
  }

  .install__meta {
    display: grid;
    gap: 10px;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-ink);
    background: var(--ark-paper);
  }

  .install__meta-title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 20px;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: -0.01em;
  }

  .install__meta-list {
    display: grid;
    gap: 8px;
    counter-reset: note;
  }

  .install__meta-list li {
    position: relative;
    padding-left: 26px;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
    color: var(--ark-ink-soft);
    counter-increment: note;
  }

  .install__meta-list li::before {
    content: counter(note, decimal-leading-zero);
    position: absolute;
    left: 0;
    top: 1px;
    font-family: var(--ark-font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--ark-accent-dim);
  }

  .install__meta-list strong {
    color: var(--ark-ink);
  }

  .install__meta-list code {
    font-family: var(--ark-font-mono);
    font-size: 0.94em;
    padding: 1px 4px;
    background: var(--ark-paper-warm);
  }

  .install__logs {
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .install__logs-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--ark-space-md);
  }

  @media (max-width: 1180px) {
    .install__grid,
    .install__actions {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
