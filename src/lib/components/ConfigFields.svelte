<script lang="ts">
  import type { Config } from '$lib/api/types';
  import { markDirty } from '$lib/stores/config.svelte.ts';
  import ConfigFieldsSecurity from './ConfigFieldsSecurity.svelte';

  interface Props {
    config: Config;
    activeGroup: string;
    disabled?: boolean;
  }

  let { config, activeGroup, disabled = false }: Props = $props();

  const whitelistText = $derived(config.install.components_whitelist.join('\n'));
  function setWhitelist(value: string): void {
    config.install.components_whitelist = value
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    markDirty();
  }

  const switchesText = $derived(config.install.install_switches.join('\n'));
  function setSwitches(value: string): void {
    config.install.install_switches = value
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    markDirty();
  }

  const multipartText = $derived(config.download.multipart_urls.join('\n'));
  const multipartActive = $derived(config.download.multipart_urls.length > 0);
  function setMultipart(value: string): void {
    config.download.multipart_urls = value
      .split('\n')
      .map((line) => line.trim().replace(/^"|"$/g, '').trim())
      .filter((line) => line.length > 0);
    markDirty();
  }
</script>

    <div class="config__fields">
      {#if activeGroup === 'general'}
        <div class="ark-field">
          <label class="ark-field__label" for="cfg-language">
            <span>界面语言</span>
            <span class="ark-field__path">general.language</span>
          </label>
          <select
            id="cfg-language"
            class="ark-select"
            bind:value={config.general.language}
            {disabled}
            onchange={markDirty}
          >
            <option value="zh-CN">zh-CN · 简体中文</option>
            <option value="en-US">en-US · English</option>
          </select>
          <p class="ark-field__hint">类型 string · 默认 "zh-CN" · 取值范围 zh-CN / en-US</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-theme">
            <span>界面风格</span>
            <span class="ark-field__path">general.theme</span>
          </label>
          <select
            id="cfg-theme"
            class="ark-select"
            bind:value={config.general.theme}
            {disabled}
            onchange={markDirty}
          >
            <option value="endfield">endfield · 白 / 炭黑 / 信号黄</option>
          </select>
          <p class="ark-field__hint">类型 string · 默认 "endfield" · 当前实现锁定 endfield 家族</p>
        </div>
      {/if}

      {#if activeGroup === 'download'}
        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-dl-multipart">
            <span>分片 URL 列表（每行一个）</span>
            <span class="ark-field__path">download.multipart_urls</span>
          </label>
          <textarea
            id="cfg-dl-multipart"
            class="ark-textarea"
            rows="4"
            spellcheck="false"
            placeholder={'https://host/sw2024.7z.001\nhttps://host/sw2024.7z.002\nhttps://host/sw2024.7z.003'}
            value={multipartText}
            {disabled}
            oninput={(event) => setMultipart(event.currentTarget.value)}
          ></textarea>
          <p class="ark-field__hint">
            类型 string[] · 默认 [] · 每行一个 URL，数量不限。非空时进入<strong>分片模式</strong>：
            各片下载到同一目录并<strong>保留原始文件名</strong>，最后把 <code>.001</code> 交给
            7z —— 7z / NanaZip 的 <code>a -v</code> 分卷会被整套自动识别，
            <strong>不要拼接</strong>。留空则使用下面的单包 URL
          </p>
          {#if multipartActive}
            <p class="ark-field__hint">
              已配置 {config.download.multipart_urls.length} 个分片：单包 URL 与 SHA-256 校验值在分片模式下不生效。
            </p>
          {/if}
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.download.multipart_concat}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">
              分片先按顺序拼接成单文件（仅非标准切分时才需要）
            </span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 false。保持 false 时直接交给 7z 处理标准分卷
            （<code>.001/.002/…</code>）；只有当分片是"被任意切开的单个文件"、
            7z 报"无法作为压缩包打开"时才需要勾选
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-dl-url">
            <span>压缩包地址（单包模式）</span>
            <span class="ark-field__path">download.url</span>
          </label>
          <input
            id="cfg-dl-url"
            class="ark-input"
            type="url"
            placeholder="https://example.com/solidworks2024sp5.7z"
            bind:value={config.download.url}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 string · 必须是 HTTP/HTTPS 直链；留空时只能使用本地模式</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-dl-threads">
            <span>下载线程数</span>
            <span class="ark-field__path">download.threads</span>
          </label>
          <input
            id="cfg-dl-threads"
            class="ark-input"
            type="number"
            min="1"
            max="255"
            step="1"
            bind:value={config.download.threads}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 integer · 默认 32 · 范围 1~255（超出会被后端钳制）·
            <strong>每个文件</strong>内部切几块
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-dl-concurrent">
            <span>同时下载文件数</span>
            <span class="ark-field__path">download.concurrent_files</span>
          </label>
          <input
            id="cfg-dl-concurrent"
            class="ark-input"
            type="number"
            min="1"
            max="32"
            step="1"
            bind:value={config.download.concurrent_files}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 integer · 默认 4 · 范围 1~32 · 分片模式下同时下几个文件。
            <strong>峰值连接数 = 线程数 × 本项</strong>（默认 32 × 4 = 128）；
            被网盘限流时优先调小本项
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.download.probe_filenames}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">
              下载前探测真实文件名（网盘直链务必保持开启）
            </span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。网盘直链的文件名常藏在查询参数里并挂着
            <code>.aspx</code> 假后缀；不探测会存成 <code>xxx.001.aspx</code>，
            而 7z 需要 <code>.001/.002</code> 这样成套的名字才能读整套分卷
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-dl-retry">
            <span>重试次数</span>
            <span class="ark-field__path">download.retry_count</span>
          </label>
          <input
            id="cfg-dl-retry"
            class="ark-input"
            type="number"
            min="0"
            max="20"
            step="1"
            bind:value={config.download.retry_count}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 integer · 默认 3 · 范围 0~20</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-dl-backoff">
            <span>重试间隔基数</span>
            <span class="ark-field__path">download.retry_backoff_minutes</span>
          </label>
          <input
            id="cfg-dl-backoff"
            class="ark-input"
            type="number"
            min="0"
            step="0.1"
            bind:value={config.download.retry_backoff_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 0.5 · 指数退避（×2 递增，上限 30 分钟）</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-dl-connect">
            <span>连接超时</span>
            <span class="ark-field__path">download.connect_timeout_minutes</span>
          </label>
          <input
            id="cfg-dl-connect"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.1"
            bind:value={config.download.connect_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 1</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-dl-read">
            <span>读取超时</span>
            <span class="ark-field__path">download.read_timeout_minutes</span>
          </label>
          <input
            id="cfg-dl-read"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.1"
            bind:value={config.download.read_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 5</p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-dl-sha">
            <span>SHA-256 校验值</span>
            <span class="ark-field__path">download.checksum_sha256</span>
          </label>
          <input
            id="cfg-dl-sha"
            class="ark-input"
            type="text"
            spellcheck="false"
            placeholder="64 位十六进制，留空则跳过校验"
            bind:value={config.download.checksum_sha256}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认 "" · 留空跳过校验；填写后校验失败会删除分片以便重新下载
          </p>
        </div>
      {/if}

      {#if activeGroup === 'remote_config'}
        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-remote-url">
            <span>远程配置 URL</span>
            <span class="ark-field__path">remote_config.url</span>
          </label>
          <input
            id="cfg-remote-url"
            class="ark-input"
            type="url"
            placeholder="https://example.com/config.toml"
            bind:value={config.remote_config.url}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 设置页「拉取远程配置」按钮直接使用该地址
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.remote_config.auto_fetch_on_start}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">启动时自动拉取远程配置</span>
          </label>
          <p class="ark-field__hint">类型 boolean · 默认 false · 开启后每次启动都会请求该 URL</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-remote-timeout">
            <span>拉取超时</span>
            <span class="ark-field__path">remote_config.fetch_timeout_minutes</span>
          </label>
          <input
            id="cfg-remote-timeout"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.1"
            bind:value={config.remote_config.fetch_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 1</p>
        </div>
      {/if}

      {#if activeGroup === 'install'}
        <div class="ark-field">
          <label class="ark-field__label" for="cfg-drive">
            <span>目标盘符</span>
            <span class="ark-field__path">install.install_drive</span>
          </label>
          <input
            id="cfg-drive"
            class="ark-input"
            type="text"
            maxlength="1"
            placeholder="C"
            bind:value={config.install.install_drive}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 string · 单个字母（C / D / E…）· 默认 "C"</p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-install-path">
            <span>自定义安装路径</span>
            <span class="ark-field__path">install.install_path</span>
          </label>
          <input
            id="cfg-install-path"
            class="ark-input"
            type="text"
            placeholder="留空则使用 [install_drive]:\SW"
            bind:value={config.install.install_path}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认 "" · 展开后的目标目录会传给 INSTALLDIR；文件替换按组件目录合并到此目录
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-whitelist">
            <span>组件白名单（每行一个）</span>
            <span class="ark-field__path">install.components_whitelist</span>
          </label>
          <textarea
            id="cfg-whitelist"
            class="ark-textarea"
            rows="4"
            spellcheck="false"
            placeholder={'SOLIDWORKS\nSimulation\nFlow Simulation'}
            value={whitelistText}
            {disabled}
            oninput={(event) => setWhitelist(event.currentTarget.value)}
          ></textarea>
          <p class="ark-field__hint">
            类型 string[] · 默认 [] · 空数组 = 全部组件。
            <strong>只对 msiexec 路径生效</strong>：StartSWInstall 读的是管理员映像里生成的
            <code>.sldIM</code>，命令行不接受组件列表 —— 走 StartSWInstall 时本项不生效，
            程序会在日志里明确告警
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.force_msiexec}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">
              强制走 msiexec（要用组件白名单时才需要打开）
            </span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 false。打开后跳过 <code>StartSWInstall.exe</code>，
            直接用 <code>msiexec ADDLOCAL=…</code> 安装 —— 代价是会跳过安装管理器负责的
            前置检查与部分组件的串接安装
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-switches">
            <span>追加给安装器的命令行开关（每行一个）</span>
            <span class="ark-field__path">install.install_switches</span>
          </label>
          <textarea
            id="cfg-switches"
            class="ark-textarea"
            rows="3"
            spellcheck="false"
            placeholder={'/l\nC:\\sw-install.log'}
            value={switchesText}
            {disabled}
            oninput={(event) => setSwitches(event.currentTarget.value)}
          ></textarea>
          <p class="ark-field__hint">
            类型 string[] · 默认 [] · 程序固定传 <code>/install /now</code>
            （<code>/now</code> 跳过 5 分钟警告对话框），这里只补你需要的额外项
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-serial">
            <span>序列号</span>
            <span class="ark-field__path">install.serial_number</span>
          </label>
          <input
            id="cfg-serial"
            class="ark-input"
            type="text"
            placeholder="留空则依赖 .reg 文件"
            bind:value={config.install.serial_number}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认 "" · 映射到 MSI 属性
            <code>SOLIDWORKSSERIALNUMBER</code>（官方属性名，不是 SERIALNUMBER）
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-toolbox">
            <span>Toolbox 数据目录</span>
            <span class="ark-field__path">install.toolbox_folder</span>
          </label>
          <input
            id="cfg-toolbox"
            class="ark-input"
            type="text"
            placeholder="C:\SOLIDWORKS Data"
            bind:value={config.install.toolbox_folder}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 映射到官方属性 <code>TOOLBOXFOLDER</code> ·
            只在组件白名单含 <code>SolidWorksToolbox</code> 时才会传
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-langpack">
            <span>语言包</span>
            <span class="ark-field__path">install.language_pack</span>
          </label>
          <select
            id="cfg-langpack"
            class="ark-select"
            bind:value={config.install.language_pack}
            {disabled}
            onchange={markDirty}
          >
            <option value="">（不安装语言包）</option>
            <option value="chinese-simplified">chinese-simplified · 简体中文 (2052)</option>
            <option value="chinese">chinese · 繁体中文 (1028)</option>
            <option value="english">english · 英语 (1033)</option>
            <option value="japanese">japanese · 日语 (1041)</option>
            <option value="korean">korean · 韩语 (1042)</option>
            <option value="german">german · 德语 (1031)</option>
            <option value="french">french · 法语 (1036)</option>
            <option value="italian">italian · 意大利语 (1040)</option>
            <option value="spanish">spanish · 西班牙语 (1034)</option>
            <option value="russian">russian · 俄语 (1049)</option>
            <option value="polish">polish · 波兰语 (1045)</option>
            <option value="czech">czech · 捷克语 (1029)</option>
            <option value="turkish">turkish · 土耳其语 (1055)</option>
            <option value="portuguese-brazilian">portuguese-brazilian · 巴西葡萄牙语 (1046)</option>
          </select>
          <p class="ark-field__hint">
            类型 string · 取值就是介质 <code>swwi\lang\</code> 下的**目录名**。
            语言包是**独立的 MSI**（官方要求单独安装）：程序会先装主程序，
            主程序进程退出后立即启动语言包，避免重复等待；
            Windows Installer 的全局锁仍会保证安装顺序
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.install_prerequisites}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">
              检测并从介质安装 Windows 前置组件（.NET 4.8 / VC++ / WebView2 / VBA）
            </span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。官方手册明确：<strong>命令行安装不会自动装前置组件</strong>
            —— 平时双击 <code>setup.exe</code> 是安装管理器代劳，改走 <code>msiexec</code>
            就必须自己补。缺失的后果不是「装不上」而是「装上了跑不起来」：
            <code>solidworks.msi</code> 没有 LaunchCondition 表，安装会报成功
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-prereq-timeout">
            <span>前置组件安装超时</span>
            <span class="ark-field__path">install.prerequisite_timeout_minutes</span>
          </label>
          <input
            id="cfg-prereq-timeout"
            class="ark-input"
            type="number"
            min="1"
            step="1"
            bind:value={config.install.prerequisite_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 float · 单位分钟 · 默认 20 · .NET 4.8 安装包有 112 MB，慢盘上要多留时间
          </p>
        </div>

      {/if}

      {#if activeGroup === 'polling'}
        <div class="ark-field">
          <label class="ark-field__label" for="cfg-poll-interval">
            <span>轮询间隔</span>
            <span class="ark-field__path">install.polling.poll_interval_minutes</span>
          </label>
          <input
            id="cfg-poll-interval"
            class="ark-input"
            type="number"
            min="0.01"
            step="0.1"
            bind:value={config.install.polling.poll_interval_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 0.5（30 秒）· 实际睡眠被限制在 0.2~5 秒之间以保证响应</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-poll-timeout">
            <span>最长等待</span>
            <span class="ark-field__path">install.polling.timeout_minutes</span>
          </label>
          <input
            id="cfg-poll-timeout"
            class="ark-input"
            type="number"
            min="1"
            step="1"
            bind:value={config.install.polling.timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 240（4 小时）· 超时即判定失败</p>
        </div>

        <div class="ark-field config__field--wide">
          <span class="ark-field__label">
            <span>检测项（至少满足两项判定完成）</span>
            <span class="ark-field__path">install.polling.*_enabled</span>
          </span>
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.polling.log_check_enabled}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">检查安装日志成功标记（安装成功 / Installation succeeded）</span>
          </label>
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.polling.process_check_enabled}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">检查安装进程是否已退出</span>
          </label>
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.polling.file_check_enabled}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">检查 SLDWORKS.exe 是否生成</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认全部 true · 只启用一项时，该项即代表全部结论
          </p>
        </div>
      {/if}

      {#if activeGroup === 'popup'}
        <div class="ark-field">
          <label class="ark-field__label" for="cfg-popup-scan">
            <span>窗口扫描间隔</span>
            <span class="ark-field__path">install.popup.scan_interval_ms</span>
          </label>
          <input
            id="cfg-popup-scan"
            class="ark-input"
            type="number"
            min="50"
            max="60000"
            step="50"
            bind:value={config.install.popup.scan_interval_ms}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 integer · 单位毫秒 · 默认 500 · 范围 50~60000（唯一按毫秒计的参数）</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-popup-timeout">
            <span>单弹窗等待超时</span>
            <span class="ark-field__path">install.popup.popup_timeout_minutes</span>
          </label>
          <input
            id="cfg-popup-timeout"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.5"
            bind:value={config.install.popup.popup_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 5 · 同标题弹窗在该窗口内只处理一次</p>
        </div>

        <div class="ark-field config__field--wide">
          <span class="ark-field__label">
            <span>自动点击策略</span>
            <span class="ark-field__path">install.popup.auto_click_*</span>
          </span>
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.popup.auto_click_confirm}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">自动点击「确定」（含"端口@服务器"确认，会写入 25734@localhost）</span>
          </label>
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.popup.auto_click_yes}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">自动点击「是」（"不能核实此服务器存在"）</span>
          </label>
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.install.popup.auto_click_no}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">自动点击「否」（默认关闭，避免误拒必要操作）</span>
          </label>
          <p class="ark-field__hint">类型 boolean · 被禁用的动作既不会点击，也不会产生事件</p>
        </div>
      {/if}

      {#if activeGroup === 'network'}
        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.network.disable_network}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">安装期间禁用物理网卡</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true · 仅禁用非虚拟网卡（跳过 VMware / Hyper-V / VPN / 蓝牙等）
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.network.restore_network_after}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">安装后恢复网络（Drop 守卫兜底）</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true · 即使流程 panic，Drop 也会重新启用适配器
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-net-method">
            <span>禁用方式</span>
            <span class="ark-field__path">network.adapter_disable_method</span>
          </label>
          <select
            id="cfg-net-method"
            class="ark-select"
            bind:value={config.network.adapter_disable_method}
            {disabled}
            onchange={markDirty}
          >
            <option value="netsh">netsh · interface set interface admin=disable</option>
            <option value="powershell">powershell · Set-NetAdapter -AdminStatus Down</option>
          </select>
          <p class="ark-field__hint">类型 string · 默认 "netsh" · 非法值会被后端归一化为 netsh</p>
        </div>
      {/if}

      {#if activeGroup === 'process'}
        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.process.kill_sw_processes}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">关闭 SolidWorks 相关进程（第 10 步）</span>
          </label>
          <p class="ark-field__hint">类型 boolean · 默认 true · 关闭后跳过进程清理与许可服务删除</p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-field__label" for="cfg-kill-pattern">
            <span>进程匹配关键词 / 正则</span>
            <span class="ark-field__path">process.kill_pattern</span>
          </label>
          <input
            id="cfg-kill-pattern"
            class="ark-input"
            type="text"
            spellcheck="false"
            placeholder="solidworks|sldworks|flexnet"
            bind:value={config.process.kill_pattern}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · <strong>不区分大小写</strong>，首尾空白自动忽略，
            <code>|</code> 分隔多个关键词。写纯关键词即可（<code>solidworks</code> 等价于
            <code>.*solidworks.*</code>）。<strong>空值会被拒绝</strong>，避免误杀全部进程；
            正则语法错误会直接报错并终止该步骤
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.process.match_command_line}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">
              同时匹配进程完整路径与命令行（能按关键词找到"名字无关"的进程）
            </span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true。开启后每轮会查询进程元数据，
            例如 <code>java.exe -jar ...\solidworks-helper.jar</code> 也能被
            <code>solidworks</code> 命中；只关心进程名时设为 false 可显著提速。
            日志会写明每个进程是"按进程名"还是"按路径/命令行"命中
          </p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-kill-timeout">
            <span>优雅退出等待</span>
            <span class="ark-field__path">process.kill_timeout_minutes</span>
          </label>
          <input
            id="cfg-kill-timeout"
            class="ark-input"
            type="number"
            min="0"
            step="0.5"
            bind:value={config.process.kill_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 2 · 超时后按下一项决定是否强杀</p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.process.force_kill_after_timeout}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">超时后强制终止（taskkill /F /T，再退回 Win32 TerminateProcess）</span>
          </label>
          <p class="ark-field__hint">类型 boolean · 默认 true</p>
        </div>
      {/if}

      {#if activeGroup === 'flexnet'}
        <div class="ark-field">
          <label class="ark-field__label" for="cfg-flex-install">
            <span>服务安装超时</span>
            <span class="ark-field__path">flexnet.server_install_timeout_minutes</span>
          </label>
          <input
            id="cfg-flex-install"
            class="ark-input"
            type="number"
            min="0.1"
            step="0.5"
            bind:value={config.flexnet.server_install_timeout_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 5 · 同时用于 server_install.bat 执行与状态轮询的总预算</p>
        </div>

        <div class="ark-field">
          <label class="ark-field__label" for="cfg-flex-check">
            <span>服务状态轮询间隔</span>
            <span class="ark-field__path">flexnet.server_check_interval_minutes</span>
          </label>
          <input
            id="cfg-flex-check"
            class="ark-input"
            type="number"
            min="0.01"
            step="0.1"
            bind:value={config.flexnet.server_check_interval_minutes}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">类型 float · 单位分钟 · 默认 0.5 · 执行 sc query "SolidWorks Flexnet Server"</p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.flexnet.remove_old_server_first}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">先运行 server_remove.bat（找不到脚本时退回 sc stop + sc delete）</span>
          </label>
          <p class="ark-field__hint">类型 boolean · 默认 true</p>
        </div>
      {/if}

      {#if activeGroup === 'workdir'}
        <div class="ark-field">
          <label class="ark-field__label" for="cfg-tempdir">
            <span>临时目录</span>
            <span class="ark-field__path">workdir.temp_dir</span>
          </label>
          <input
            id="cfg-tempdir"
            class="ark-input"
            type="text"
            placeholder="留空则用 [app_data_dir]\work"
            bind:value={config.workdir.temp_dir}
            {disabled}
            oninput={markDirty}
          />
          <p class="ark-field__hint">
            类型 string · 默认 "" · 非空时实际工作目录为 [temp_dir]\solidworks-install，需要 ≥ 60 GB 空间
          </p>
        </div>

        <div class="ark-field config__field--wide">
          <label class="ark-switch">
            <input
              type="checkbox"
              bind:checked={config.workdir.cleanup_temp_after}
              {disabled}
              onchange={markDirty}
            />
            <span class="ark-switch__track"></span>
            <span class="ark-switch__text">安装完成后清理临时文件（第 13 步）</span>
          </label>
          <p class="ark-field__hint">
            类型 boolean · 默认 true · 排查失败原因时建议改为 false 以保留解压产物
          </p>
        </div>
      {/if}

      <ConfigFieldsSecurity {config} {disabled} {activeGroup} />
    </div>


<style>
  .config__fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    align-content: start;
    gap: var(--ark-space-md) var(--ark-space-md);
    padding: var(--ark-space-md);
    min-width: 0;
  }

  .config__field--wide {
    grid-column: 1 / -1;
  }
</style>
