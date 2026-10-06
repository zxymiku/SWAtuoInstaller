<script lang="ts">
  /**
   * 分区舞台：maximal 深度下最多 6 层协同视觉层。
   *
   *   1 工程网格      2 长方向引导线   3 45° 切入楔形
   *   4 校准刻度尺    5 超大分区编号   6 细扫描带
   *
   * 每层都有明确的信息角色（坐标、方向、刻度、区块标识），
   * 不是无意义的 HUD 噪声；层的不透明度由 `data-ark-depth` 变量统一控制。
   */
  interface Props {
    /** 超大编号文本（例如 "01"）；留空则不渲染 */
    identifier?: string;
    /** 是否显示扫描带（只有 maximal 会真正显现） */
    scan?: boolean;
  }

  let { identifier = '', scan = true }: Props = $props();
</script>

<div class="ark-stage__layers" aria-hidden="true">
  <div class="ark-stage__grid"></div>
  <div class="ark-stage__guide ark-stage__guide--a"></div>
  <div class="ark-stage__guide ark-stage__guide--b"></div>
  <div class="ark-stage__wedge ark-stage__wedge--a"></div>
  <div class="ark-stage__wedge ark-stage__wedge--b"></div>
  <div class="ark-stage__calibration"></div>
  {#if identifier}
    <div class="ark-stage__identifier">{identifier}</div>
  {/if}
  {#if scan}
    <div class="ark-stage__scan"></div>
  {/if}
</div>
