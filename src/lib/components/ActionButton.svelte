<script lang="ts">
  /**
   * 行动按钮：方形 + 左侧信号条 + 1px 描边。
   * 保留可见动词与说明文字，不做仅图标的提交/破坏性操作。
   */
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    /** 次级说明，显示在按钮下方或作为 title */
    hint?: string;
    variant?: 'default' | 'primary' | 'danger' | 'dock';
    size?: 'default' | 'compact';
    disabled?: boolean;
    type?: 'button' | 'submit';
    title?: string;
    onclick?: (event: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    label,
    hint = '',
    variant = 'default',
    size = 'default',
    disabled = false,
    type = 'button',
    title = '',
    onclick,
    children,
  }: Props = $props();

  const variantClass = $derived(
    variant === 'primary'
      ? 'ark-btn--primary'
      : variant === 'danger'
        ? 'ark-btn--danger'
        : variant === 'dock'
          ? 'ark-btn--dock'
          : '',
  );
</script>

<button
  {type}
  class="ark-btn {variantClass}"
  class:ark-btn--compact={size === 'compact'}
  {disabled}
  title={title || hint || label}
  aria-label={label}
  onclick={onclick}
>
  <span class="ark-btn__label">{label}</span>
  {#if children}
    {@render children()}
  {/if}
</button>
