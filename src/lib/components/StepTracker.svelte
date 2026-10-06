<script lang="ts">
  /**
   * 步骤追踪器：9 个逻辑步骤，大型编号 + 状态驱动着色。
   *
   * 状态语义：
   *   done    已完成（信号黄填充编号块）
   *   active  进行中（炭黑底 + 黄色活动条 + 呼吸指示）
   *   pending 未开始（米白 + 细描边）
   */
  import type { StepDefinition } from '$lib/stores/install.svelte.ts';

  interface Props {
    steps: StepDefinition[];
    /** 1 基的当前步骤序号；0 表示未开始 */
    currentStep: number;
    /** 已完成的步骤下标（0 基） */
    completedSteps: number[];
    /** 是否正在运行 */
    running: boolean;
    /** 点击某一步时回调（用于滚动到日志锚点） */
    onSelect?: (step: StepDefinition) => void;
  }

  let { steps, currentStep, completedSteps, running, onSelect }: Props = $props();

  function stateOf(index1: number): 'done' | 'active' | 'pending' {
    if (currentStep > 0 && index1 === currentStep) return 'active';
    if (completedSteps.includes(index1 - 1)) return 'done';
    if (currentStep > index1) return 'done';
    return 'pending';
  }
</script>

<ol class="stepper" aria-label="安装步骤">
  {#each steps as step (step.id)}
    {@const state = stateOf(step.index)}
    <li class="stepper__item" data-state={state}>
      <button
        type="button"
        class="stepper__button"
        aria-current={state === 'active' ? 'step' : undefined}
        onclick={() => onSelect?.(step)}
      >
        <span class="stepper__rail" aria-hidden="true">
          <span class="stepper__node"></span>
        </span>

        <span class="stepper__index" aria-hidden="true">
          {String(step.index).padStart(2, '0')}
        </span>

        <span class="stepper__body">
          <span class="stepper__name">{step.name}</span>
          <span class="stepper__en">{step.enName}</span>
          <span class="stepper__note">{step.note}</span>
        </span>

        <span class="stepper__state">
          {#if state === 'done'}
            <span class="stepper__badge" data-badge="done">完成</span>
          {:else if state === 'active'}
            <span class="stepper__badge" data-badge="active">
              <i aria-hidden="true"></i>
              {running ? '进行中' : '当前'}
            </span>
          {:else}
            <span class="stepper__badge" data-badge="pending">待执行</span>
          {/if}
        </span>
      </button>
    </li>
  {/each}
</ol>

<style>
  .stepper {
    display: grid;
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
  }

  .stepper__item + .stepper__item .stepper__button {
    border-top: var(--ark-rule) solid var(--ark-border);
  }

  .stepper__button {
    position: relative;
    display: grid;
    grid-template-columns: 10px 62px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--ark-space-sm);
    width: 100%;
    padding: 10px var(--ark-space-md) 10px 0;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
    transition: background var(--ark-motion-direct) var(--ark-ease);
  }

  .stepper__button:hover {
    background: var(--ark-paper-warm);
  }

  /* 左侧纵向导轨：把 9 个步骤串成一条长引导线 */
  .stepper__rail {
    position: relative;
    align-self: stretch;
    display: grid;
    place-items: center;
  }

  .stepper__rail::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 4px;
    width: var(--ark-rule);
    background: var(--ark-border);
  }

  .stepper__item:first-child .stepper__rail::before {
    top: 50%;
  }

  .stepper__item:last-child .stepper__rail::before {
    bottom: 50%;
  }

  .stepper__node {
    position: relative;
    z-index: 1;
    width: 9px;
    height: 9px;
    background: var(--ark-paper);
    border: var(--ark-rule) solid var(--ark-ink-mute);
  }

  .stepper__index {
    font-family: var(--ark-font-display);
    font-size: 30px;
    font-weight: 800;
    line-height: 1;
    letter-spacing: -0.04em;
    color: var(--ark-paper-deep);
    transition: color var(--ark-motion-direct) var(--ark-ease);
  }

  .stepper__body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .stepper__name {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.01em;
  }

  .stepper__en {
    font-family: var(--ark-font-mono);
    font-size: 9px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .stepper__note {
    font-size: var(--ark-font-size-micro);
    line-height: 1.5;
    color: var(--ark-ink-mute);
  }

  .stepper__badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 68px;
    justify-content: flex-end;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
    white-space: nowrap;
  }

  .stepper__badge[data-badge='done'] {
    color: var(--ark-level-success);
    font-weight: 700;
  }

  .stepper__badge[data-badge='active'] {
    color: var(--ark-ink);
    font-weight: 700;
  }

  .stepper__badge i {
    width: 8px;
    height: 8px;
    background: var(--ark-accent);
    animation: ark-breathe var(--ark-motion-attention) var(--ark-ease) infinite;
  }

  /* ---------- 状态着色 ---------- */

  .stepper__item[data-state='done'] .stepper__index {
    color: var(--ark-ink);
  }

  .stepper__item[data-state='done'] .stepper__node {
    background: var(--ark-ink);
    border-color: var(--ark-ink);
  }

  .stepper__item[data-state='active'] .stepper__button {
    background: var(--ark-ink);
    color: var(--ark-paper);
  }

  .stepper__item[data-state='active'] .stepper__button::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 5px;
    background: var(--ark-accent);
  }

  .stepper__item[data-state='active'] .stepper__index {
    color: var(--ark-accent);
  }

  .stepper__item[data-state='active'] .stepper__en,
  .stepper__item[data-state='active'] .stepper__note {
    color: rgb(242 242 240 / 66%);
  }

  .stepper__item[data-state='active'] .stepper__node {
    background: var(--ark-accent);
    border-color: var(--ark-accent);
  }

  .stepper__item[data-state='active'] .stepper__badge[data-badge='active'] {
    color: var(--ark-accent);
  }

  .stepper__item[data-state='pending'] .stepper__name {
    color: var(--ark-ink-mute);
    font-weight: 600;
  }

  @media (max-width: 720px) {
    .stepper__button {
      grid-template-columns: 10px 44px minmax(0, 1fr);
      padding-right: var(--ark-space-sm);
    }

    .stepper__index {
      font-size: 22px;
    }

    .stepper__state {
      grid-column: 3;
      justify-self: start;
      margin-top: 4px;
    }

    .stepper__note {
      display: none;
    }
  }
</style>
