<script lang="ts">
  /**
   * 安全软件处置面板。
   *
   * 三种状态，各有明确的可操作出口：
   *
   * 1. **已处置** —— 列出探测到的全部安全软件与各自结论。
   * 2. **需要手动退出** —— 高亮提示需要用户去系统托盘退出的软件，
   *    并给出该软件自己的 `manual_hint`（不是笼统的"请关闭杀软"）。
   * 3. **需要重启** —— Defender 已移除但驱动/服务注册要重启才彻底消失；
   *    这里明确告知，并提供「重启前先让安装继续」的说明。
   *
   * 面板只呈现后端回传的真实结论，不做"看起来成功"的假设。
   */
  import StatusChip from './StatusChip.svelte';
  import ActionButton from './ActionButton.svelte';
  import type { AvOutcome, SecurityProduct } from '$lib/api/types';

  interface Props {
    report: {
      detected: SecurityProduct[];
      outcomes: AvOutcome[];
      manualRequired: string[];
      defenderRemoved: boolean;
      rebootRequired: boolean;
      summary: string;
    } | null;
    /** 安装是否正在进行（决定能否重试） */
    running: boolean;
    /** 用户点「我已手动退出，重新检测」 */
    onRecheck: () => void;
    /** 用户确认已手动关闭或卸载安全软件。 */
    onAcknowledgeResolved: () => void;
  }

  let { report, running, onRecheck, onAcknowledgeResolved }: Props = $props();

  const hasReport = $derived(report !== null);
  const needManual = $derived((report?.manualRequired.length ?? 0) > 0);

  /** 需要用户处理的条目的完整指引。 */
  const manualHints = $derived(
    (report?.outcomes ?? []).filter((item) => !item.resolved && item.manual_hint),
  );
</script>

{#if hasReport}
  <section
    class="av"
    data-state={needManual ? 'action-required' : 'resolved'}
    aria-labelledby="av-title"
  >
    <header class="av__head">
      <div>
        <p class="av__index">SECURITY SOFTWARE / ANTIVIRUS</p>
        <h3 id="av-title" class="av__title">
          {needManual ? '需要你手动退出安全软件' : '安全软件已处置'}
        </h3>
      </div>
      <StatusChip
        label={needManual ? '待处理' : '已就绪'}
        value={needManual ? `${report?.manualRequired.length ?? 0} 个` : '可继续'}
        state={needManual ? 'warn' : 'ok'}
      />
    </header>

    <p class="av__summary">{report?.summary}</p>

    {#if needManual}
      <div class="av__blocker" role="alert">
        <p class="av__blocker-lead">
          以下安全软件<strong>无法自动停用</strong>。它们的实时防护会把你解压出来的补丁文件
          当成威胁<strong>直接删除</strong>，导致安装走到第 11 步「文件替换」时缺少源文件。
        </p>
        <ol class="av__steps">
          <li>在 Windows 右下角<strong>系统托盘</strong>找到该软件的图标（可能藏在 <kbd>^</kbd> 里）</li>
          <li>右键图标 → 选择「<strong>退出</strong>」或「<strong>暂停防护 / 关闭实时监控</strong>」</li>
          <li>若托盘没有图标：打开它的主界面关闭实时防护，或在「设置 → 应用」中卸载</li>
          <li>完成后点下方按钮重新检测</li>
        </ol>
      </div>
    {/if}

    <div class="av__table-wrap ark-scroll">
      <table class="av__table">
        <caption class="ark-visually-hidden">检测到的安全软件与处置结论</caption>
        <thead>
          <tr>
            <th scope="col">安全软件</th>
            <th scope="col">类型</th>
            <th scope="col">实时防护</th>
            <th scope="col">处置结果</th>
          </tr>
        </thead>
        <tbody>
          {#each (report?.detected ?? []) as product (product.display_name)}
            {@const outcome = (report?.outcomes ?? []).find(
              (item) => item.display_name.toLowerCase() === product.display_name.toLowerCase(),
            )}
            <tr data-resolved={outcome ? String(outcome.resolved) : 'unknown'}>
              <td class="av__name">{product.display_name}</td>
              <td>{product.is_defender ? '内置' : '第三方'}</td>
              <td>
                <span class="av__flag" data-on={product.enabled}>
                  {product.enabled ? '开启' : '关闭'}
                </span>
              </td>
              <td>
                {#if outcome}
                  <span class="av__verdict" data-ok={outcome.resolved}>
                    {outcome.resolved ? '已停用' : '未能停用'}
                  </span>
                  <span class="av__action">{outcome.action}</span>
                {:else}
                  <span class="av__action">未处置（按配置跳过）</span>
                {/if}
              </td>
            </tr>
          {/each}
          {#if (report?.detected ?? []).length === 0}
            <tr>
              <td colspan="4" class="av__empty">
                未能从 SecurityCenter2 读出任何安全软件（可能查询被策略禁用）。
                若你已知机器上装了杀软，请手动确认它已退出。
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>

    {#if manualHints.length > 0}
      <ul class="av__hints">
        {#each manualHints as item (item.display_name)}
          <li>
            <strong>{item.display_name}</strong>
            <span>{item.manual_hint}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if report?.rebootRequired}
      <div class="av__reboot" role="status">
        <p class="av__reboot-title">已移除 Windows Defender —— 需要重启才彻底生效</p>
        <p>
          策略注册表与安全中心应用已经处理完，但<strong>驱动与服务注册要重启后才会消失</strong>。
          本程序<strong>不会替你重启</strong>。
        </p>
        <p>
          建议做法：<strong>先让本次安装继续</strong>（Defender 已经不会再删文件），
          等安装结束后再重启系统；重启后可以到「Windows 安全中心」确认它确实已不存在。
        </p>
      </div>
    {/if}

    <footer class="av__foot">
      <ActionButton
        label="我已手动解决"
        size="compact"
        variant="primary"
        disabled={!needManual}
        title="确认已关闭或卸载杀毒软件，不等待重新检测"
        onclick={onAcknowledgeResolved}
      />
      <ActionButton
        label={running ? '重新检测安全软件' : '重新检测（未在安装中）'}
        size="compact"
        variant={needManual ? 'primary' : 'default'}
        disabled={!running}
        title={running ? '重新扫描 SecurityCenter2，确认安全软件是否已退出' : '安装未在进行，无法重检'}
        onclick={onRecheck}
      />
      <p class="av__foot-note">
        重检只读取系统状态，不会重复执行移除动作。
      </p>
    </footer>
  </section>
{/if}

<style>
  .av {
    display: grid;
    gap: 10px;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-state-ok);
    background: var(--ark-paper);
    min-width: 0;
  }

  .av[data-state='action-required'] {
    border-left-color: var(--ark-state-warn);
    background: var(--ark-warn-wash);
  }

  .av__head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--ark-space-md);
    flex-wrap: wrap;
  }

  .av__index {
    margin: 0 0 4px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .av__title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 20px;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: -0.01em;
  }

  .av[data-state='action-required'] .av__title {
    color: var(--ark-level-warn);
  }

  .av__summary {
    margin: 0;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
    color: var(--ark-ink-soft);
  }

  .av__blocker {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border: var(--ark-rule) solid var(--ark-state-warn);
    background: var(--ark-paper);
  }

  .av__blocker-lead {
    margin: 0;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
  }

  .av__blocker-lead strong {
    color: var(--ark-level-warn);
  }

  .av__steps {
    display: grid;
    gap: 4px;
    counter-reset: av-step;
  }

  .av__steps li {
    position: relative;
    padding-left: 24px;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
    color: var(--ark-ink-soft);
    counter-increment: av-step;
  }

  .av__steps li::before {
    content: counter(av-step, decimal-leading-zero);
    position: absolute;
    left: 0;
    top: 1px;
    font-family: var(--ark-font-mono);
    font-size: 10px;
    color: var(--ark-accent-dim);
  }

  .av__steps kbd {
    padding: 0 4px;
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper-warm);
    font-family: var(--ark-font-mono);
    font-size: 10px;
  }

  .av__table-wrap {
    max-height: 260px;
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
  }

  .av__table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }

  .av__table th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 8px 10px;
    border-bottom: var(--ark-rule) solid var(--ark-ink);
    background: var(--ark-paper-warm);
    font-family: var(--ark-font-mono);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    text-align: left;
    color: var(--ark-ink-mute);
  }

  .av__table td {
    padding: 8px 10px;
    border-bottom: var(--ark-rule) solid var(--ark-border);
    vertical-align: top;
  }

  .av__table tr:last-child td {
    border-bottom: 0;
  }

  .av__name {
    font-weight: 700;
    word-break: break-word;
  }

  .av__flag,
  .av__verdict {
    display: inline-block;
    padding: 1px 6px;
    font-family: var(--ark-font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
  }

  .av__flag[data-on='true'] {
    background: var(--ark-error-wash);
    color: var(--ark-level-error);
  }

  .av__flag[data-on='false'] {
    background: var(--ark-ok-wash);
    color: var(--ark-level-success);
  }

  .av__verdict[data-ok='true'] {
    background: var(--ark-ok-wash);
    color: var(--ark-level-success);
    font-weight: 700;
  }

  .av__verdict[data-ok='false'] {
    background: var(--ark-warn-wash);
    color: var(--ark-level-warn);
    font-weight: 700;
  }

  .av__action {
    display: block;
    margin-top: 4px;
    font-size: var(--ark-font-size-micro);
    line-height: 1.6;
    color: var(--ark-ink-mute);
  }

  .av__empty {
    padding: var(--ark-space-md) !important;
    color: var(--ark-ink-mute);
    text-align: center;
  }

  .av__hints {
    display: grid;
    gap: 6px;
  }

  .av__hints li {
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border-left: 3px solid var(--ark-state-warn);
    background: var(--ark-paper);
    font-size: var(--ark-font-size-micro);
    line-height: 1.7;
    color: var(--ark-ink-soft);
  }

  .av__hints strong {
    font-size: var(--ark-font-size-label);
    color: var(--ark-ink);
  }

  .av__reboot {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border: var(--ark-rule) solid var(--ark-ink);
    border-left-width: 3px;
    background: var(--ark-paper-warm);
  }

  .av__reboot p {
    margin: 0;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
    color: var(--ark-ink-soft);
  }

  .av__reboot-title {
    font-weight: 700;
    color: var(--ark-ink) !important;
  }

  .av__foot {
    display: flex;
    align-items: center;
    gap: var(--ark-space-md);
    flex-wrap: wrap;
    padding-top: 8px;
    border-top: var(--ark-rule) solid var(--ark-border);
  }

  .av__foot-note {
    margin: 0;
    font-size: var(--ark-font-size-micro);
    color: var(--ark-ink-mute);
  }
</style>
