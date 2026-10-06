<script lang="ts">
  /**
   * 设置页。
   *
   * 五个真实动作：
   *   · 拉取远程配置 —— URL 取自 `config[remote_config].url`，只填表不落盘
   *   · 重置默认     —— 恢复嵌入二进制的 default-config.toml
   *   · 保存配置     —— 结构保留合并后写入 `{app_data_dir}/config.toml`
   *   · 导出当前配置 —— 写出一份 TOML（留空项保留磁盘现有值），位置可自选
   *   · 查看默认值   —— 展开嵌入的 TOML 原文，便于对照
   */
  import ActionButton from '$lib/components/ActionButton.svelte';
  import ConfigPanel from '$lib/components/ConfigPanel.svelte';
  import EnvironmentPanel from '$lib/components/EnvironmentPanel.svelte';
  import SectionTitle from '$lib/components/SectionTitle.svelte';
  import StatusChip from '$lib/components/StatusChip.svelte';
  import { getDefaultConfig, logUiEvent } from '$lib/api/commands';
  import {
    config,
    configRuntime,
    environments,
    exportCurrentConfig,
    loadConfig,
    loadEnvironment,
    persistConfig,
    pullRemoteConfig,
    resetToDefault,
    setNotice,
  } from '$lib/stores/config.svelte.ts';
  import { installState } from '$lib/stores/install.svelte.ts';

  interface Props {
    onNavigate: (id: string) => void;
  }

  let { onNavigate }: Props = $props();

  const busy = $derived(
    configRuntime.loading ||
      configRuntime.saving ||
      configRuntime.fetching ||
      configRuntime.resetting,
  );

  const noticeState = $derived(
    configRuntime.notice?.kind === 'ok'
      ? 'ok'
      : configRuntime.notice?.kind === 'error'
        ? 'error'
        : 'idle',
  );

  async function toggleDefaultToml(): Promise<void> {
    void logUiEvent(
      configRuntime.defaultToml === null ? '点击：查看默认配置' : '点击：收起默认配置',
    );
    if (configRuntime.defaultToml !== null) {
      configRuntime.defaultToml = null;
      void logUiEvent('响应：默认配置已收起');
      return;
    }
    try {
      configRuntime.defaultToml = await getDefaultConfig();
      void logUiEvent('响应：默认配置读取成功');
    } catch (error) {
      setNotice(
        'error',
        `读取默认配置失败: ${error instanceof Error ? error.message : String(error)}`,
      );
      void logUiEvent(`响应：默认配置读取失败；${error instanceof Error ? error.message : String(error)}`);
    }
  }
  const configPathLabel = $derived(
    environments.snapshot?.config_path ??
      (configRuntime.hostAvailable ? '读取中…' : '未连接宿主'),
  );
</script>

<div class="settings">
  <SectionTitle
    index="CONFIGURATION / 02"
    title="参数配置"
    caption="TOML · 11 GROUPS · TIME IN MINUTES"
    note="所有可调参数集中在 config.toml。默认配置嵌入二进制，用户配置存储在 [app_data_dir]/config.toml；保存时使用 toml_edit 结构保留合并，保留你已有的值与注释，并自动补齐新增字段。"
    scale="page"
  />

  <div class="settings__toolbar">
    <div class="settings__toolbar-actions">
      <ActionButton
        label={configRuntime.fetching ? '拉取中…' : '拉取远程配置'}
        variant="primary"
        disabled={busy}
        title={`从 ${config.remote_config.url || '（未设置 URL）'} 拉取配置`}
        onclick={() => pullRemoteConfig()}
      />
      <ActionButton
        label={configRuntime.resetting ? '重置中…' : '重置默认'}
        disabled={busy}
        onclick={resetToDefault}
      />
      <ActionButton
        label={configRuntime.saving ? '保存中…' : '保存配置'}
        disabled={busy || !configRuntime.dirty}
        onclick={persistConfig}
      />
      <ActionButton
        label="导出当前配置"
        disabled={busy}
        title="把当前配置写成 TOML 文件：表单里留空的项保持磁盘上的现有值，注释保留、新字段补齐"
        onclick={exportCurrentConfig}
      />
      <ActionButton
        label={configRuntime.defaultToml === null ? '查看默认值' : '收起默认值'}
        disabled={busy}
        onclick={toggleDefaultToml}
      />
      <ActionButton
        label="重新读取"
        disabled={busy}
        onclick={() => {
          void logUiEvent('点击：重新读取配置');
          void loadConfig().then(() => loadEnvironment());
        }}
      />
    </div>

    <div class="settings__toolbar-readouts">
      <StatusChip
        label="未保存修改"
        value={configRuntime.dirty ? '有' : '无'}
        state={configRuntime.dirty ? 'warn' : 'ok'}
        title="与最近一次加载/保存的配置比对结果"
      />
      <StatusChip
        label="最近同步"
        value={
          configRuntime.lastSyncedAt
            ? configRuntime.lastSyncedAt.slice(11, 19)
            : '未同步'
        }
      />
      <StatusChip
        label="远程 URL"
        value={config.remote_config.url ? '已配置' : '为空'}
        state={config.remote_config.url ? 'ok' : 'warn'}
        title={config.remote_config.url || '请在 [remote_config] 中填写'}
      />
    </div>
  </div>

  {#if configRuntime.notice}
    <p
      class="settings__notice"
      data-kind={configRuntime.notice.kind}
      role={configRuntime.notice.kind === 'error' ? 'alert' : 'status'}
    >
      <span class="settings__notice-mark" aria-hidden="true">
        {configRuntime.notice.kind === 'ok'
          ? '✓'
          : configRuntime.notice.kind === 'error'
            ? '×'
            : 'i'}
      </span>
      <span>{configRuntime.notice.text}</span>
    </p>
  {/if}

  {#if installState.running}
    <p class="settings__notice" data-kind="info" role="status">
      <span class="settings__notice-mark" aria-hidden="true">i</span>
      <span>
        部署正在进行中，配置为只读。需要修改请先取消部署，避免安装中途行为不一致。
      </span>
    </p>
  {/if}

  <ConfigPanel {config} disabled={installState.running || busy} />

  <div class="settings__split">
    <EnvironmentPanel
      snapshot={environments.snapshot}
      loading={environments.loading}
      error={environments.error}
      onRefresh={loadEnvironment}
    />

    <section class="settings__hint" aria-labelledby="settings-hint-title">
      <p class="settings__index">SAVE SEMANTICS</p>
      <h3 id="settings-hint-title" class="settings__hint-title">保存行为说明</h3>
      <ol>
        <li>
          <strong>保存配置</strong>：先与磁盘上的现有文档合并，新字段带上默认注释写入，
          你手写的注释与自定义值不会被丢弃。
        </li>
        <li>
          <strong>拉取远程配置</strong>：远端 TOML 只补齐缺失字段，结果直接填入表单；
          需要点「保存配置」才会写盘。
        </li>
        <li>
          <strong>重置默认</strong>：用嵌入二进制的 default-config.toml 覆盖用户文件，
          此操作不可撤销。
        </li>
        <li>
          <strong>开始部署时</strong>：主页会把当前表单中的配置再归一化并落盘一次，
          保证本次运行与下次启动读到的配置一致。
        </li>
      </ol>

      <div class="settings__foot">
        <ActionButton
          label="返回部署页"
          variant="primary"
          onclick={() => {
            void logUiEvent('点击：返回部署页');
            onNavigate('install');
          }}
        />
        <ActionButton
          label={configRuntime.saving ? '保存中…' : '保存并返回部署页'}
          disabled={busy || !configRuntime.dirty}
          onclick={async () => {
            if (await persistConfig()) onNavigate('install');
          }}
        />
      </div>
    </section>
  </div>

  {#if configRuntime.defaultToml !== null}
    <section class="settings__toml" aria-labelledby="settings-toml-title">
      <header class="settings__toml-head">
        <div>
          <p class="settings__index">EMBEDDED DEFAULT / include_str!</p>
          <h3 id="settings-toml-title" class="settings__hint-title">嵌入式默认配置原文</h3>
        </div>
        <ActionButton label="收起" size="compact" onclick={toggleDefaultToml} />
      </header>
      <pre class="settings__toml-body ark-scroll"><code>{configRuntime.defaultToml}</code></pre>
    </section>
  {/if}

  <p class="settings__signature" data-state={noticeState}>
    配置文件：<code>{configPathLabel}</code>
  </p>
</div>

<style>
  .settings {
    display: grid;
    gap: var(--ark-space-lg);
    min-width: 0;
  }

  .settings__toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--ark-space-md);
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-accent);
    background: var(--ark-paper);
  }

  .settings__toolbar-actions,
  .settings__toolbar-readouts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ark-space-sm);
    align-items: center;
  }

  .settings__notice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin: 0;
    padding: 10px 12px;
    border: var(--ark-rule) solid var(--ark-border);
    border-left-width: 3px;
    background: var(--ark-paper-warm);
    font-size: var(--ark-font-size-label);
    line-height: 1.65;
  }

  .settings__notice[data-kind='ok'] {
    border-left-color: var(--ark-state-ok);
    color: var(--ark-level-success);
    background: var(--ark-ok-wash);
  }

  .settings__notice[data-kind='error'] {
    border-left-color: var(--ark-state-error);
    color: var(--ark-state-error);
    background: var(--ark-error-wash);
  }

  .settings__notice[data-kind='info'] {
    border-left-color: var(--ark-ink);
  }

  .settings__notice-mark {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    background: currentColor;
    color: var(--ark-paper);
    font-family: var(--ark-font-mono);
    font-size: 12px;
    font-weight: 700;
  }

  .settings__split {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--ark-space-lg);
    align-items: start;
    min-width: 0;
  }

  .settings__index {
    margin: 0 0 4px;
    font-family: var(--ark-font-mono);
    font-size: var(--ark-font-size-micro);
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--ark-ink-mute);
  }

  .settings__hint {
    display: grid;
    gap: 10px;
    align-content: start;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-border);
    border-left: 3px solid var(--ark-ink);
    background: var(--ark-paper);
  }

  .settings__hint-title {
    margin: 0;
    font-family: var(--ark-font-display);
    font-size: 20px;
    font-weight: 800;
    line-height: 1;
    text-transform: uppercase;
    letter-spacing: -0.01em;
  }

  .settings__hint ol {
    display: grid;
    gap: 8px;
    counter-reset: hint;
  }

  .settings__hint li {
    position: relative;
    padding-left: 26px;
    font-size: var(--ark-font-size-label);
    line-height: 1.7;
    color: var(--ark-ink-soft);
    counter-increment: hint;
  }

  .settings__hint li::before {
    content: counter(hint, decimal-leading-zero);
    position: absolute;
    left: 0;
    top: 1px;
    font-family: var(--ark-font-mono);
    font-size: 10px;
    color: var(--ark-accent-dim);
  }

  .settings__hint strong {
    color: var(--ark-ink);
  }

  .settings__foot {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ark-space-sm);
    padding-top: var(--ark-space-sm);
    border-top: var(--ark-rule) solid var(--ark-border);
  }

  .settings__toml {
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .settings__toml-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--ark-space-md);
  }

  .settings__toml-body {
    max-height: 420px;
    margin: 0;
    padding: var(--ark-space-md);
    border: var(--ark-rule) solid var(--ark-ink);
    background: var(--ark-dock);
    color: var(--ark-dock-text);
    font-family: var(--ark-font-mono);
    font-size: 12px;
    line-height: 1.7;
    overflow: auto;
  }

  .settings__signature {
    margin: 0;
    font-size: var(--ark-font-size-micro);
    color: var(--ark-ink-mute);
  }

  .settings__signature code {
    font-family: var(--ark-font-mono);
    color: var(--ark-ink-soft);
  }
</style>
