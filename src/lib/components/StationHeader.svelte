<script lang="ts">
  /**
   * 碳黑器具坞站（页头）：品牌标记 + 当前任务 + 实时状态读数。
   * 状态全部来自真实后端数据，不制造装饰性遥测。
   */
  import StatusChip from './StatusChip.svelte';
  import type { StepId } from '$lib/api/types';

  interface Props {
    /** 当前页面标识（微标签） */
    routeLabel: string;
    /** 当前正在执行的步骤名；null 表示待命 */
    activeStepName: string | null;
    activeStepId: StepId | null;
    /** 运行中 */
    running: boolean;
    /** 进度百分比 */
    percent: number;
    /** 已处理的弹窗数 */
    popups: number;
    /** 网络是否已禁用 */
    networkDisabled: boolean;
    /** 镜像是否已挂载 */
    isoMounted: boolean;
    /** 是否以管理员运行 */
    elevated: boolean;
    /** 主机是否可用 */
    hostAvailable: boolean;
  }

  let {
    routeLabel,
    activeStepName,
    activeStepId,
    running,
    percent,
    popups,
    networkDisabled,
    isoMounted,
    elevated,
    hostAvailable,
  }: Props = $props();

  const runState = $derived(running ? 'active' : 'idle');
</script>

<header class="ark-dock station-head">
  <div class="station-head__brand">
    <span class="station-head__mark" aria-hidden="true"></span>
    <span class="station-head__names">
      <strong>SolidWorks 2024 SP5</strong>
      <small>DEPLOYMENT TERMINAL / {routeLabel}</small>
    </span>
  </div>

  <div class="station-head__task" data-empty={activeStepName === null}>
    <span class="station-head__task-label">当前任务</span>
    <span class="station-head__task-value">
      {activeStepName ?? '待命 / STANDBY'}
    </span>
    {#if activeStepId}
      <span class="station-head__task-code">{activeStepId.toUpperCase()}</span>
    {/if}
  </div>

  <div class="station-head__readouts">
    <StatusChip
      onDock
      label="宿主"
      value={hostAvailable ? '已连接' : '未连接'}
      state={hostAvailable ? 'ok' : 'error'}
      title={hostAvailable ? 'Tauri IPC 通道可用' : '未检测到 Tauri 宿主'}
    />
    <StatusChip
      onDock
      label="权限"
      value={elevated ? '管理员' : '受限'}
      state={elevated ? 'ok' : 'warn'}
      title={elevated ? '已获得管理员权限' : '缺少管理员权限，注册表与服务操作会失败'}
    />
    <StatusChip
      onDock
      label="网络"
      value={networkDisabled ? '已隔离' : '正常'}
      state={networkDisabled ? 'warn' : 'ok'}
      title="安装期间按配置禁用/恢复物理网卡"
    />
    <StatusChip
      onDock
      label="镜像"
      value={isoMounted ? '已挂载' : '未挂载'}
      state={isoMounted ? 'active' : 'idle'}
      title="ISO 挂载状态，收尾步骤会自动卸载"
    />
    <StatusChip
      onDock
      label="弹窗"
      value={String(popups)}
      state={popups > 0 ? 'active' : 'idle'}
      title="弹窗守护线程已自动处理的对话框数量"
    />
    <StatusChip
      onDock
      label="进度"
      value={`${percent.toFixed(1)}%`}
      state={runState}
      title="整体流程进度"
    />
  </div>
</header>

<style>
  .station-head {
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(0, 1fr) minmax(0, auto);
    align-items: center;
    gap: var(--ark-space-md);
    padding: 0 clamp(var(--ark-space-md), 2vw, var(--ark-space-lg));
    background: var(--ark-dock);
    color: var(--ark-dock-text);
    border-bottom: 3px solid var(--ark-accent);
  }

  .station-head__brand {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .station-head__mark {
    position: relative;
    width: 26px;
    height: 26px;
    flex: 0 0 auto;
    background: var(--ark-accent);
    clip-path: polygon(0 0, 100% 0, 0 100%);
  }

  .station-head__names {
    display: block;
    min-width: 0;
  }

  .station-head__names strong {
    display: block;
    font-family: var(--ark-font-display);
    font-size: 16px;
    font-weight: 800;
    line-height: 1;
    letter-spacing: -0.01em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .station-head__names small {
    display: block;
    margin-top: 4px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.16em;
    color: var(--ark-dock-muted);
    white-space: nowrap;
  }

  .station-head__task {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
    padding-left: var(--ark-space-md);
    border-left: var(--ark-rule) solid var(--ark-dock-rule);
  }

  .station-head__task-label {
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.14em;
    color: var(--ark-dock-muted);
    text-transform: uppercase;
    white-space: nowrap;
  }

  .station-head__task-value {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .station-head__task[data-empty='true'] .station-head__task-value {
    color: var(--ark-dock-muted);
    font-weight: 500;
  }

  .station-head__task-code {
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.12em;
    color: var(--ark-accent);
    white-space: nowrap;
  }

  .station-head__readouts {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: flex-end;
    min-width: 0;
  }

  @media (max-width: 1280px) {
    .station-head__task {
      display: none;
    }

    .station-head {
      grid-template-columns: minmax(0, 1fr) minmax(0, auto);
    }
  }

  @media (max-width: 900px) {
    .station-head {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--ark-space-sm);
      padding-block: 10px;
    }

    .station-head__readouts {
      flex-wrap: wrap;
      justify-content: flex-start;
    }
  }
</style>
