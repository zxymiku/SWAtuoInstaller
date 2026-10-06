<script lang="ts">
/**
 * 应用外壳：endfield 导轨 + 炭黑坞站 + 分区舞台。
 *
 * 键盘与路由：
 *   · 单一 `#/` `#/settings` `#/logs` 哈希路由，Tauri 的 file:// 资源下可直接刷新；
 *   · 侧栏为语义化 `<nav>`，活动项使用 `aria-current="page"`（不只靠颜色）；
 *   · 页面切换时把焦点移到主标题，屏幕阅读器能感知上下文变化。
 */

import { onMount } from 'svelte';

import RailNav from '$lib/components/RailNav.svelte';
import StageField from '$lib/components/StageField.svelte';
import StationHeader from '$lib/components/StationHeader.svelte';
import {
  configRuntime,
  loadConfig,
  loadEnvironment,
} from '$lib/stores/config.svelte.ts';
import { installState, refreshStatus } from '$lib/stores/install.svelte.ts';

import InstallPage from './routes/InstallPage.svelte';
import LogsPage from './routes/LogsPage.svelte';
import SettingsPage from './routes/SettingsPage.svelte';

type RouteId = 'install' | 'settings' | 'logs';

const RAIL_ITEMS = [
  { id: 'install', index: '01', label: '部署', hint: 'DEPLOY' },
  { id: 'settings', index: '02', label: '设置', hint: 'CONFIG' },
  { id: 'logs', index: '03', label: '日志', hint: 'ARCHIVE' },
];

const ROUTE_LABELS: Record<RouteId, string> = {
  install: 'INSTALL WIZARD',
  settings: 'CONFIGURATION',
  logs: 'LOG ARCHIVE',
};

/** 各页面的超大分区编号（舞台第 5 层）。 */
const ROUTE_IDENTIFIERS: Record<RouteId, string> = {
  install: '01',
  settings: '02',
  logs: '03',
};

function parseHash(hash: string): RouteId {
  const cleaned = hash.replace(/^#\/?/, '').split('?')[0].toLowerCase();
  if (cleaned === 'settings') return 'settings';
  if (cleaned === 'logs') return 'logs';
  return 'install';
}

let hash = $state(typeof window === 'undefined' ? '#/' : window.location.hash);
let route = $derived(parseHash(hash));
let mainElement = $state<HTMLElement | null>(null);
let wipe = $state(false);

function navigate(id: string): void {
  const next = `#/${id === 'install' ? '' : id}`;
  if (window.location.hash === next) {
    hash = next;
    return;
  }
  window.location.hash = next;
}

onMount(() => {
  const onHashChange = () => {
    hash = window.location.hash;
  };
  window.addEventListener('hashchange', onHashChange);

  // 首屏拉取配置、环境与（可能正在进行的）安装状态。
  void loadConfig().then(() => loadEnvironment());
  void refreshStatus();

  return () => window.removeEventListener('hashchange', onHashChange);
});

/** 路由变化：播放一次黄色擦入，并把焦点交给主标题。 */
$effect(() => {
  void route;
  wipe = false;
  const frame = requestAnimationFrame(() => {
    wipe = true;
    const heading = mainElement?.querySelector<HTMLElement>('[data-page-title]');
    heading?.focus({ preventScroll: false });
  });
  const timer = window.setTimeout(() => {
    wipe = false;
  }, 700);
  return () => {
    cancelAnimationFrame(frame);
    window.clearTimeout(timer);
  };
});

const railNote = $derived([
  configRuntime.hostAvailable ? 'HOST ONLINE' : 'HOST OFFLINE',
  installState.running ? 'DEPLOY ACTIVE' : 'STANDBY',
  `STEP ${installState.currentStep || 0}/9`,
]);
</script>

<div class="shell" data-route={route}>
  <a class="ark-skip-link" href="#main-stage">跳到主内容</a>

  <div class="shell__rail">
    <RailNav items={RAIL_ITEMS} active={route} onNavigate={navigate} note={railNote} />
  </div>

  <div class="shell__head">
    <StationHeader
      routeLabel={ROUTE_LABELS[route]}
      activeStepName={installState.running || installState.finished
        ? installState.message
        : null}
      activeStepId={installState.currentStepId}
      running={installState.running}
      percent={installState.percent}
      popups={installState.popupsHandled}
      networkDisabled={installState.networkDisabled}
      isoMounted={installState.isoMounted}
      elevated={configRuntime.hostAvailable}
      hostAvailable={configRuntime.hostAvailable}
    />
  </div>

  <main id="main-stage" class="shell__stage ark-stage" bind:this={mainElement}>
    <StageField identifier={ROUTE_IDENTIFIERS[route]} />

    <div class="shell__viewport ark-wipe" data-wipe={wipe}>
      {#if route === 'install'}
        <InstallPage onNavigate={navigate} />
      {:else if route === 'settings'}
        <SettingsPage onNavigate={navigate} />
      {:else}
        <LogsPage />
      {/if}
    </div>
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: var(--ark-rail) minmax(0, 1fr);
    grid-template-rows: var(--ark-topbar) minmax(0, 1fr);
    min-height: 100svh;
    background: var(--ark-paper);
  }

  .shell__rail {
    grid-row: 1 / -1;
    min-height: 0;
  }

  .shell__head {
    grid-column: 2;
    min-width: 0;
  }

  .shell__stage {
    grid-column: 2;
    min-width: 0;
    min-height: 0;
    padding: clamp(var(--ark-space-md), 2vw, var(--ark-space-lg));
    overflow: hidden;
  }

  .shell__viewport {
    position: relative;
    min-width: 0;
  }

  @media (max-width: 860px), (orientation: portrait) and (max-width: 1024px) {
    .shell {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr) auto;
    }

    .shell__rail {
      grid-row: 3;
    }

    .shell__head,
    .shell__stage {
      grid-column: 1;
    }

    .shell__stage {
      padding: var(--ark-space-md);
    }
  }
</style>
