<script lang="ts">
  /**
   * 分区标题：大型编号 + 双语副标题 + 伸入负空间的长引导线。
   * 装饰全部由伪元素/独立元素承担，标题文本保持语义化。
   */
  interface Props {
    /** 展示编号，例如 "02 / 09" */
    index: string;
    /** 主标题（中文） */
    title: string;
    /** 次标题（英文微标签） */
    caption?: string;
    /** 标题右侧的补充说明 */
    note?: string;
    /** 标题规模：page 用于页头，section 用于面板内 */
    scale?: 'page' | 'section';
  }

  let { index, title, caption = '', note = '', scale = 'section' }: Props = $props();
</script>

<header class="ark-title" data-scale={scale}>
  <p class="ark-title__index">{index}</p>
  <div class="ark-title__line">
    <h2 class="ark-title__text">{title}</h2>
    {#if caption}
      <span class="ark-title__caption">{caption}</span>
    {/if}
  </div>
  {#if note}
    <p class="ark-title__note">{note}</p>
  {/if}
  <span class="ark-title__rule" aria-hidden="true"></span>
</header>

<style>
  .ark-title {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .ark-title__index {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.2em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .ark-title__line {
    display: flex;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: 0 var(--ark-space-md);
    min-width: 0;
  }

  .ark-title__text {
    margin: 0;
    font-family: var(--ark-font-display);
    font-weight: 800;
    line-height: 0.88;
    letter-spacing: -0.03em;
    text-transform: uppercase;
    color: var(--ark-ink);
    white-space: nowrap;
  }

  .ark-title[data-scale='page'] .ark-title__text {
    font-size: clamp(34px, 4.4vw, 58px);
  }

  .ark-title[data-scale='section'] .ark-title__text {
    font-size: clamp(20px, 1.7vw, 26px);
  }

  .ark-title__caption {
    padding-bottom: 0.4em;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .ark-title__note {
    margin: 0;
    max-width: 68ch;
    font-size: var(--ark-font-size-label);
    line-height: 1.65;
    color: var(--ark-ink-mute);
  }

  .ark-title__rule {
    position: relative;
    display: block;
    height: var(--ark-rule);
    margin-top: 10px;
    background: var(--ark-ink);
  }

  /* 黄色切口：给长引导线一个方向性终止点 */
  .ark-title__rule::after {
    content: '';
    position: absolute;
    right: 0;
    top: -3px;
    width: 26px;
    height: 7px;
    background: var(--ark-accent);
  }
</style>
