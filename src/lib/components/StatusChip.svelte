<script lang="ts">
  /**
   * 状态胶囊：一个真实的「标签 + 值」读数对。
   * 颜色只是辅助，`data-state` 同时驱动圆点与无障碍语义。
   */
  interface Props {
    label: string;
    value: string;
    state?: 'idle' | 'ok' | 'warn' | 'error' | 'active';
    /** 用于深色器械坞站内 */
    onDock?: boolean;
    title?: string;
  }

  let { label, value, state = 'idle', onDock = false, title = '' }: Props = $props();
</script>

<span
  class="ark-chip"
  class:ark-chip--dock={onDock}
  data-state={state}
  title={title || `${label}: ${value}`}
>
  <i class="ark-chip__dot" aria-hidden="true"></i>
  <span class="ark-chip__label">{label}</span>
  <span class="ark-chip__value">{value}</span>
</span>
