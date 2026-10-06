<script lang="ts">
  /**
   * 浅色竖向导轨：图标栈 + 大型编号 + 活动信号条。
   *
   * 活动态不只靠颜色：同时使用填充、左侧信号条与字重，
   * 保证在灰度或高对比模式下依然可辨。
   * 竖排（writing-mode）只用于次要装饰标签，主要标签保持横排可读。
   */
  interface RailItem {
    id: string;
    index: string;
    label: string;
    hint: string;
  }

  interface Props {
    items: RailItem[];
    active: string;
    onNavigate: (id: string) => void;
    /** 侧栏底部的状态注记 */
    note?: string[];
  }

  let { items, active, onNavigate, note = [] }: Props = $props();
</script>

<nav class="rail" aria-label="主导航">
  <ul class="rail__list">
    {#each items as item (item.id)}
      {@const isActive = item.id === active}
      <li>
        <button
          type="button"
          class="rail__item"
          aria-current={isActive ? 'page' : undefined}
          onclick={() => onNavigate(item.id)}
        >
          <span class="rail__index" aria-hidden="true">{item.index}</span>
          <span class="rail__label">{item.label}</span>
          <span class="rail__hint">{item.hint}</span>
        </button>
      </li>
    {/each}
  </ul>

  {#if note.length > 0}
    <footer class="rail__note">
      {#each note as line, i (line)}
        <span>{line}</span>{#if i < note.length - 1}<br />{/if}
      {/each}
    </footer>
  {/if}
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    border-right: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
  }

  .rail__list {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .rail__item {
    position: relative;
    display: grid;
    justify-items: center;
    align-content: center;
    gap: 6px;
    width: 100%;
    min-height: 104px;
    padding: var(--ark-space-sm) 6px;
    border: 0;
    border-bottom: var(--ark-rule) solid var(--ark-border);
    background: transparent;
    color: var(--ark-ink-mute);
    cursor: pointer;
    text-align: center;
    transition:
      background var(--ark-motion-direct) var(--ark-ease),
      color var(--ark-motion-direct) var(--ark-ease);
  }

  .rail__item::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    background: transparent;
    transition: background var(--ark-motion-direct) var(--ark-ease);
  }

  .rail__item:hover {
    background: var(--ark-paper-warm);
    color: var(--ark-ink);
  }

  .rail__index {
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.14em;
    color: var(--ark-accent-dim);
    transition: color var(--ark-motion-direct) var(--ark-ease);
  }

  .rail__label {
    font-family: var(--ark-font-display);
    font-size: 15px;
    font-weight: 800;
    line-height: 1;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .rail__hint {
    font-family: var(--ark-font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
    writing-mode: horizontal-tb;
  }

  .rail__item[aria-current='page'] {
    background: var(--ark-ink);
    color: var(--ark-paper);
  }

  .rail__item[aria-current='page']::before {
    background: var(--ark-accent);
  }

  .rail__item[aria-current='page'] .rail__index {
    color: var(--ark-accent);
  }

  .rail__item[aria-current='page'] .rail__hint {
    color: rgb(242 242 240 / 62%);
  }

  .rail__note {
    margin: auto 0 0;
    padding: var(--ark-space-md) 8px;
    font-family: var(--ark-font-mono);
    font-size: 9px;
    line-height: 1.9;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
    border-top: var(--ark-rule) solid var(--ark-border);
  }

  @media (max-width: 860px), (orientation: portrait) and (max-width: 1024px) {
    .rail {
      flex-direction: row;
      height: auto;
      border-right: 0;
      border-top: var(--ark-rule) solid var(--ark-border);
    }

    .rail__list {
      flex-direction: row;
      flex: 1;
      min-width: 0;
    }

    .rail__list > li {
      flex: 1;
      min-width: 0;
    }

    .rail__item {
      min-height: 62px;
      border-bottom: 0;
      border-right: var(--ark-rule) solid var(--ark-border);
      padding: 8px 4px;
    }

    .rail__item::before {
      inset: auto 0 0 0;
      width: auto;
      height: 3px;
    }

    .rail__hint {
      display: none;
    }

    .rail__note {
      display: none;
    }
  }
</style>
