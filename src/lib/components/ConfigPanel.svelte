<script lang="ts">
  /**
   * 配置面板：分组折叠 + 全量字段编辑。
   *
   * 布局是「左侧分区索引 + 右侧表单舞台」的器械布局：
   * 左侧给出 9 个分组的编号与已完成度，右侧只渲染当前分组，
   * 避免把所有字段堆成一堵墙。
   *
   * 每个字段都标注 `[分组].键名`、单位与取值范围——
   * 时间参数在界面上统一按「分钟」显示，与 TOML 保持一致。
   */
  import ActionButton from './ActionButton.svelte';
  import type { Config } from '$lib/api/types';
  import ConfigFields from './ConfigFields.svelte';

  interface Props {
    config: Config;
    /** 只读（安装进行中时） */
    disabled?: boolean;
  }

  let { config, disabled = false }: Props = $props();

  type GroupId =
    | 'general'
    | 'download'
    | 'remote_config'
    | 'install'
    | 'polling'
    | 'popup'
    | 'network'
    | 'process'
    | 'flexnet'
    | 'workdir'
    | 'sevenzip'
    | 'antivirus';

  interface GroupMeta {
    id: GroupId;
    index: string;
    label: string;
    caption: string;
    description: string;
  }

  const GROUPS: GroupMeta[] = [
    {
      id: 'general',
      index: '01',
      label: '通用',
      caption: '[general]',
      description: '界面语言与风格。风格固定为 endfield 家族。',
    },
    {
      id: 'download',
      index: '02',
      label: '下载',
      caption: '[download]',
      description:
        '多线程分块下载：线程数 1~255，所有时间参数单位为分钟。校验值留空则跳过 SHA-256。',
    },
    {
      id: 'remote_config',
      index: '03',
      label: '远程配置',
      caption: '[remote_config]',
      description:
        '远程配置拉取地址与超时。拉取结果只填入表单，需确认后保存才会写入本地。',
    },
    {
      id: 'install',
      index: '04',
      label: '安装',
      caption: '[install]',
      description:
        '目标盘符与安装路径。install_path 留空时使用 [install_drive]:\SW；组件白名单为空表示安装全部组件。',
    },
    {
      id: 'polling',
      index: '05',
      label: '轮询检测',
      caption: '[install.polling]',
      description:
        '安装完成判定：日志、进程、可执行文件三项中至少满足两项即视为完成。',
    },
    {
      id: 'popup',
      index: '06',
      label: '弹窗处理',
      caption: '[install.popup]',
      description:
        '窗口守护线程的扫描间隔（毫秒）与自动点击策略。安装期间按此配置处理对话框。',
    },
    {
      id: 'network',
      index: '07',
      label: '网络管理',
      caption: '[network]',
      description:
        '安装期间禁用物理网卡，收尾或异常退出时由 Drop 守卫恢复。虚拟网卡不会被触碰。',
    },
    {
      id: 'process',
      index: '08',
      label: '进程管理',
      caption: '[process]',
      description:
        '用正则匹配 SolidWorks 相关进程：先优雅关闭，超时后强制终止。',
    },
    {
      id: 'flexnet',
      index: '09',
      label: 'FlexNet 服务',
      caption: '[flexnet]',
      description:
        '运行 server_remove.bat / server_install.bat 并轮询 "SolidWorks Flexnet Server" 服务状态。',
    },
    {
      id: 'workdir',
      index: '10',
      label: '工作目录',
      caption: '[workdir]',
      description: '解压与临时文件的落点；安装完成后是否清理。',
    },
    {
      id: 'sevenzip',
      index: '11',
      label: '7z 解压',
      caption: '[sevenzip]',
      description:
        'exe_path 留空时按「配置 → 打包资源 → 下载缓存 → 已安装的 7-Zip → 第三方实现 → PATH」查找。',
    },
    {
      id: 'antivirus',
      index: '12',
      label: '安全软件',
      caption: '[antivirus]',
      description:
        'Defender 与多数第三方杀软会把 _SolidSQUAD_ 里的补丁判为威胁并删除，所以本阶段在**解压之前**执行：移除 Defender、尽量停用第三方杀软；停不掉的会在部署页明确提示你手动从系统托盘退出。',
    },
  ];

  let activeGroup = $state<GroupId>('general');
  const activeMeta = $derived(GROUPS.find((group) => group.id === activeGroup) ?? GROUPS[0]);

</script>

<div class="config">
  <nav class="config__index" aria-label="配置分组">
    <p class="config__index-head">配置分组 / GROUPS</p>
    <ul>
      {#each GROUPS as group (group.id)}
        <li>
          <button
            type="button"
            class="config__group-btn"
            aria-current={activeGroup === group.id ? 'true' : undefined}
            onclick={() => {
              activeGroup = group.id;
            }}
          >
            <span class="config__group-index">{group.index}</span>
            <span class="config__group-label">{group.label}</span>
            <span class="config__group-code">{group.caption}</span>
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <section class="config__stage" aria-labelledby="config-group-title">
    <header class="config__stage-head">
      <p class="config__stage-index">{activeMeta.index} / {String(GROUPS.length).padStart(2, '0')}</p>
      <h3 id="config-group-title" class="config__stage-title">{activeMeta.label}</h3>
      <code class="config__stage-code">{activeMeta.caption}</code>
      <p class="config__stage-desc">{activeMeta.description}</p>
    </header>

    <ConfigFields {config} {disabled} {activeGroup} />
    <footer class="config__stage-foot">
      <ActionButton
        label="上一个分组"
        size="compact"
        disabled={GROUPS.findIndex((group) => group.id === activeGroup) === 0}
        onclick={() => {
          const index = GROUPS.findIndex((group) => group.id === activeGroup);
          if (index > 0) activeGroup = GROUPS[index - 1].id;
        }}
      />
      <span class="config__stage-counter">
        {GROUPS.findIndex((group) => group.id === activeGroup) + 1} / {GROUPS.length}
      </span>
      <ActionButton
        label="下一个分组"
        size="compact"
        variant="primary"
        disabled={GROUPS.findIndex((group) => group.id === activeGroup) === GROUPS.length - 1}
        onclick={() => {
          const index = GROUPS.findIndex((group) => group.id === activeGroup);
          if (index < GROUPS.length - 1) activeGroup = GROUPS[index + 1].id;
        }}
      />
    </footer>
  </section>
</div>

<style>
  .config {
    display: grid;
    grid-template-columns: minmax(180px, 232px) minmax(0, 1fr);
    gap: 0;
    border: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper);
    min-width: 0;
  }

  .config__index {
    border-right: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper-warm);
    min-width: 0;
  }

  .config__index-head {
    margin: 0;
    padding: 10px var(--ark-space-md);
    border-bottom: var(--ark-rule) solid var(--ark-border);
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .config__group-btn {
    position: relative;
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr);
    align-items: baseline;
    gap: 2px var(--ark-space-sm);
    width: 100%;
    min-height: 52px;
    padding: 9px var(--ark-space-md) 9px 12px;
    border: 0;
    border-bottom: var(--ark-rule) solid var(--ark-border);
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition:
      background var(--ark-motion-direct) var(--ark-ease),
      color var(--ark-motion-direct) var(--ark-ease);
  }

  .config__group-btn::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    background: transparent;
  }

  .config__group-btn:hover {
    background: var(--ark-paper-deep);
  }

  .config__group-btn[aria-current='true'] {
    background: var(--ark-ink);
    color: var(--ark-paper);
  }

  .config__group-btn[aria-current='true']::before {
    background: var(--ark-accent);
  }

  .config__group-index {
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.1em;
    color: var(--ark-accent-dim);
  }

  .config__group-btn[aria-current='true'] .config__group-index {
    color: var(--ark-accent);
  }

  .config__group-label {
    font-size: 14px;
    font-weight: 700;
  }

  .config__group-code {
    grid-column: 2;
    font-family: var(--ark-font-mono);
    font-size: 9px;
    letter-spacing: 0.08em;
    color: var(--ark-ink-mute);
  }

  .config__group-btn[aria-current='true'] .config__group-code {
    color: rgb(242 242 240 / 60%);
  }

  .config__stage {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    min-width: 0;
  }

  .config__stage-head {
    position: relative;
    display: grid;
    gap: 4px;
    padding: var(--ark-space-md);
    border-bottom: var(--ark-rule) solid var(--ark-border);
    overflow: hidden;
  }

  .config__stage-head::after {
    content: '';
    position: absolute;
    top: 0;
    right: 0;
    width: 72px;
    height: 72px;
    background: var(--ark-accent);
    clip-path: polygon(100% 0, 100% 100%, 0 0);
    opacity: calc(0.9 * var(--ark-orchestration));
  }

  .config__stage-index {
    margin: 0;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    color: var(--ark-ink-mute);
  }

  .config__stage-title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 26px;
    font-weight: 800;
    line-height: 1;
    letter-spacing: -0.02em;
    text-transform: uppercase;
  }

  .config__stage-code {
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-label);
    color: var(--ark-accent-dim);
  }

  .config__stage-desc {
    margin: 4px 0 0;
    max-width: 76ch;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
    color: var(--ark-ink-mute);
  }

  .config__stage-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ark-space-sm);
    padding: 10px var(--ark-space-md);
    border-top: var(--ark-rule) solid var(--ark-border);
    background: var(--ark-paper-warm);
  }

  .config__stage-counter {
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.14em;
    color: var(--ark-ink-mute);
  }

  @media (max-width: 900px) {
    .config {
      grid-template-columns: minmax(0, 1fr);
    }

    .config__index {
      border-right: 0;
      border-bottom: var(--ark-rule) solid var(--ark-border);
    }

    .config__index ul {
      display: flex;
      flex-wrap: wrap;
    }

    .config__index li {
      flex: 1 1 132px;
      min-width: 0;
    }

    .config__group-btn {
      border-right: var(--ark-rule) solid var(--ark-border);
      min-height: 46px;
    }
  }
</style>
