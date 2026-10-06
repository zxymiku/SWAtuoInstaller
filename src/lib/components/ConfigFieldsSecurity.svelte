<script lang="ts">
  import type { Config } from '$lib/api/types';
  import { markDirty } from '$lib/stores/config.svelte.ts';
  interface Props { config: Config; activeGroup: string; disabled?: boolean; }
  let { config, activeGroup, disabled = false }: Props = $props();
</script>

      {#if activeGroup === 'sevenzip'}
        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-7z-path">
            <span>7z.exe 路径</span>
            <span class="ark-field__path">sevenzip.exe_path</span>
          </label>
          <input
            id="cfg-7z-path"
            class="ark-input"
            type="text"
            placeholder="留空则自动查找"
            bind:value={config.sevenzip.exe_path}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认 "" · 留空时按顺序查找：配置路径 → 随程序打包的资源 →
            应用数据目录的下载缓存 → 注册表中已安装的 7-Zip → 系统 PATH。
            全部失败会在第 4 步抛出明确错误并终止
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.sevenzip.auto_download}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">找不到 7z 时自动下载（离线环境请关闭）</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true · 下载结果缓存到 [app_data_dir]\7zr.exe，下次启动直接复用
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-7z-url">
            <span>自动下载地址</span>
            <span class="ark-field__path">sevenzip.download_url</span>
          </label>
          <input
            id="cfg-7z-url"
            class="ark-input"
            type="url"
            spellcheck="false"
            bind:value={config.sevenzip.download_url}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认指向 7-Zip 官方发布的独立控制台程序 7zr.exe
            （LGPL，单文件约 600 KB）。<strong>必须是直链</strong>：GitHub 的
            releases/latest/download 是直链，releases/latest 页面不是。
            下载后会校验文件头是否为 MZ，避免把 HTML 错误页存成程序
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-7z-dl-timeout">
            <span>自动下载超时</span>
            <span class="ark-field__path">sevenzip.download_timeout_minutes</span>
          </label>
          <input
            id="cfg-7z-dl-timeout"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.5"
            bind:value={config.sevenzip.download_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 3</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-7z-timeout">
            <span>单次解压超时</span>
            <span class="ark-field__path">sevenzip.extract_timeout_minutes</span>
          </label>
          <input
            id="cfg-7z-timeout"
            class="ark-input"
            type="number"
            min="0.1"
            step="1"
            bind:value={config.sevenzip.extract_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 float · 单位分钟 · 默认 30 · 两阶段解压各自独立计时；
            该值同时被复用为网卡开关、注册表导入、ISO 挂载/卸载的等待预算
          </p>
        </div>
      {/if}

      {#if activeGroup === 'antivirus'}
        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.antivirus.enabled}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">启用安全软件处置阶段（在解压之前执行）</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。关闭后补丁文件可能被 Defender 或第三方杀软直接删除，
            安装会在第 11 步「文件替换」时缺少源文件
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.antivirus.detect}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">用 SecurityCenter2 检测并列出已装的安全软件</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。这是 Windows 安全中心自己的注册表，
            比枚举服务或猜进程名可靠；查不到时会如实提示"未能读出"而不是假装没有
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.antivirus.remove_defender}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">移除 Windows Defender（复用仓库里的 windows-defender-remover）</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。执行的是与该项目 <code>Script_Run.ps1</code> 相同的动作
            （移除安全中心应用 + 导入策略注册表 + 删除 SmartScreen 文件），
            但<strong>刻意不执行它最后的强制重启</strong> —— 重启交给你自己决定
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-av-tool">
            <span>windows-defender-remover 的 script 目录</span>
            <span class="ark-field__path">antivirus.defender_tool_path</span>
          </label>
          <input
            id="cfg-av-tool"
            class="ark-input"
            type="text"
            placeholder="留空则自动查找"
            bind:value={config.antivirus.defender_tool_path}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认 "" · 必须指向含 <code>PowerRun.exe</code> 与
            <code>RemoveSecHealthApp.ps1</code> 的 <code>script</code> 目录。
            留空时按顺序查找：配置 → exe 同级的
            <code>othertools\windows-defender-remover-main\script</code> → 资源目录 → 源码目录。
            两样文件缺一就会认为指错了地方
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.antivirus.warn_reboot_after_removal}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">移除 Defender 后提示「必须重启才彻底生效」</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。删除动作本身已完成，但驱动与服务注册要重启才消失；
            <strong>本程序不会自动重启，也不会因此中断安装</strong>
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-av-mode">
            <span>第三方杀软处置方式</span>
            <span class="ark-field__path">antivirus.third_party_mode</span>
          </label>
          <select
            id="cfg-av-mode"
            class="ark-select"
            bind:value={config.antivirus.third_party_mode}
            {disabled}
            onchange={markDirty}
          >
            <option value="prompt">prompt · 只检测并提示你手动从托盘退出</option>
            <option value="terminate">terminate · 额外终止其进程 / 停止其服务</option>
            <option value="uninstall">uninstall · 再额外尝试静默卸载</option>
          </select>
          <p class="ark-field__hint">
            类型 string · 默认 "terminate" · 逐级加强。
            <strong>uninstall 会尝试卸载别人的安全软件</strong>，请确认你确实要这样做。
            无论哪种方式，只要它仍注册为活动防护，界面就会提示你手动退出
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-av-settle">
            <span>处置后等待生效</span>
            <span class="ark-field__path">antivirus.settle_minutes</span>
          </label>
          <input
            id="cfg-av-settle"
            class="ark-input"
            type="number"
            min="0"
            step="0.5"
            bind:value={config.antivirus.settle_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 0.5（30 秒）</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-av-timeout">
            <span>单条处置命令超时</span>
            <span class="ark-field__path">antivirus.command_timeout_minutes</span>
          </label>
          <input
            id="cfg-av-timeout"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.5"
            bind:value={config.antivirus.command_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 float · 单位分钟 · 默认 5 · 用于 PowerRun 调用与 regedit 导入
          </p>
        </div>
      {/if}
