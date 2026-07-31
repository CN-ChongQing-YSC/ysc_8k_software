// pdf-doc.js — build a self-contained HTML document (usage + protocol commands +
// statuses + V1/V2 consistency) from the i18n `docs` object, ready for Electron's
// webContents.printToPDF. Used by DocsPanel's "导出 PDF" button.
//
// The `docs` object is the locale-specific content from i18n/docs/{zh,en}.js.
// Notes inside `docs` already carry inline HTML (rendered with v-html in the UI),
// so they are inserted as-is; plain strings (formats, titles, params) are escaped.

function esc(s) {
  return String(s == null ? '' : s)
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

const CSS = `
@page { margin: 14mm; }
* { box-sizing: border-box; }
body { font-family: -apple-system, "Microsoft YaHei", "Segoe UI", Arial, sans-serif;
       font-size: 10.5pt; color: #1a1a1a; line-height: 1.55; margin: 0; }
h1 { font-size: 21pt; margin: 0 0 3pt; color: #14266b; }
h2 { font-size: 14.5pt; border-bottom: 2px solid #2a5bd7; padding-bottom: 3pt;
     margin: 22pt 0 8pt; color: #1a3a8a; }
h3 { font-size: 12pt; margin: 14pt 0 5pt; color: #234; }
h4 { font-size: 10.8pt; margin: 8pt 0 3pt; color: #2a5bd7; }
p { margin: 4pt 0; }
table { border-collapse: collapse; width: 100%; margin: 6pt 0; font-size: 9.6pt; }
th, td { border: 1px solid #b8b8b8; padding: 3.5pt 6pt; text-align: left; vertical-align: top; }
th { background: #eef3fc; font-weight: 600; }
tr:nth-child(even) td { background: #f7f9fd; }
code { font-family: Consolas, "Courier New", monospace; background: #f0f1f4;
       padding: 0.5pt 4pt; border-radius: 3px; font-size: 9.2pt; color: #b3261e; }
pre { background: #f4f5f7; border: 1px solid #e0e2e6; padding: 7pt 9pt; border-radius: 4px;
      font-family: Consolas, "Courier New", monospace; font-size: 9.2pt;
      white-space: pre-wrap; word-break: break-all; margin: 5pt 0; }
.note { color: #333; margin: 3pt 0 5pt; }
.meta { color: #666; font-size: 9.3pt; margin: 0 0 10pt; }
.card { border: 1px solid #d8dae0; border-radius: 5px; padding: 7pt 10pt; margin: 6pt 0;
        page-break-inside: avoid; }
ul { margin: 4pt 0; padding-left: 17pt; }
li { margin: 2pt 0; }
.badge { display: inline-block; background: #2a5bd7; color: #fff; border-radius: 3px;
         padding: 0.5pt 5pt; font-size: 8.6pt; margin-right: 4pt; }
.kv { width: 26%; font-weight: 600; white-space: nowrap; }
.toc { background: #f7f9fd; border: 1px solid #e0e6f0; border-radius: 5px; padding: 8pt 14pt;
       font-size: 9.8pt; }
.toc a { color: #2a5bd7; text-decoration: none; }
.cover { border-left: 5px solid #2a5bd7; padding: 4pt 0 4pt 12pt; margin-bottom: 6pt; }
`;

function table(headers, rows) {
  let h = '<table><thead><tr>';
  headers.forEach((x) => { h += '<th>' + esc(x) + '</th>'; });
  h += '</tr></thead><tbody>';
  rows.forEach((r) => {
    h += '<tr>';
    r.forEach((c, i) => { h += '<td' + (i === 0 ? ' class="kv"' : '') + '>' + c + '</td>'; });
    h += '</tr>';
  });
  return h + '</tbody></table>';
}

// ---- Static sections (Chinese; the device/driver are Chinese-facing) ----

function usageSection() {
  return '' +
    '<h2>一、盒子使用</h2>' +
    '<div class="card">' +
    '<h4>这个盒子是干什么的</h4>' +
    '<p>简单说,它就是一根"中间线":你的鼠标/键盘先插到盒子上,盒子再插到电脑。' +
    '电脑会以为自己直接接着你的鼠标/键盘(用起来完全一样),但中间多了盒子——' +
    '这样就能在软件里做<strong>压枪、连点宏、鼠标曲线、抖动</strong>等调整。</p>' +
    '</div>' +
    '<div class="card">' +
    '<h4>怎么接线</h4>' +
    '<ul>' +
    '<li><strong>老款(单芯片盒子)</strong>:只有<strong>一个 USB 口</strong>,直接插到电脑就行;' +
    '你的鼠标/键盘插到盒子另一侧的口上。</li>' +
    '<li><strong>新款(双芯片盒子 / 8K V2)</strong>:有<strong>两个 USB 口</strong>,分左右——' +
    '把<strong>你的真鼠标/键盘</strong>插到盒子<strong>右边</strong>的口;' +
    '再用一根线把盒子<strong>左边</strong>的口插到<strong>电脑</strong>。' +
    '简单记:右边接真鼠标,左边接电脑。</li>' +
    '<li>接好以后,电脑上除了多出一个鼠标,还会多出一个"串口(COM 口)"——' +
    '驱动软件就是靠这个口给盒子下命令的。两边哪个插错了,要么鼠标不动,要么电脑认不出。</li>' +
    '</ul></div>' +
    '<div class="card">' +
    '<h4>驱动软件里都有哪些面板</h4>' +
    table(['面板', '能干什么'], [
      ['串口连接', '选好盒子那个串口、点连接。连上之后才能给盒子下命令'],
      ['KmNet', '可以换一台电脑、通过网络来控制盒子;带一个自检'],
      ['监控', '实时显示盒子报上来的鼠标按键和移动(要先点"开启上传")'],
      ['宏 / 抖动 / 鼠标曲线 / 手柄映射', '图形化设置压枪、连点、手感等,设好点保存就生效'],
      ['文档', '就是这份说明,外加一个命令测试器(能直接发命令看盒子怎么回)'],
      ['固件更新(老款)', '给老款单芯片盒子升级程序,不用烧录器'],
      ['8K V2 固件更新', '给新款双芯片盒子升级(左右两边各升一次)'],
      ['调试', '给开发/排错用的抓包工具,普通用户一般用不到'],
    ]) +
    '</div>' +
    '<div class="card">' +
    '<h4>上手四步</h4>' +
    '<ol style="padding-left:17pt">' +
    '<li>盒子接好线、插上电脑,打开驱动软件。</li>' +
    '<li>在「串口连接」里选盒子的串口,点连接(连上后右上角会显示已连接)。</li>' +
    '<li>想确认连没连上:打开「监控」点"开启上传",鼠标动一动看有没有数据;或在「文档」的命令测试器里发个命令看看盒子回不回。</li>' +
    '<li>要调压枪/宏/曲线就去对应面板设好保存;要升级程序就去对应固件面板(老款用「固件更新」,新款用「8K V2 固件更新」)。</li>' +
    '</ol></div>';
}

function statusSection() {
  return '' +
    '<h2>四、设备状态与响应</h2>' +
    '<h3>4.1 运行模式</h3>' +
    table(['模式', '说明', '如何识别'], [
      ['APP', '正常运行应用程序(透传/命令处理)', '查询版本 cmd:132 返回应用版本串'],
      ['IAP', 'Bootloader 等待固件升级', 'cmd:132 返回 <code>YSC-IAP</code>;V2 以序列号 <code>TOWMCUIAP</code> 枚举'],
    ]) +
    '<h3>4.2 版本字符串</h3>' +
    table(['版本', '含义'], [
      ['<code>ysc-towmcu-L v1.0</code>', 'V2 左侧 APP(对 PC 的设备侧)'],
      ['<code>ysc-towmcu-R v1.0</code>', 'V2 右侧 APP(读真实设备的主机侧)'],
      ['<code>YSC-IAP</code>', 'IAP Bootloader(V1/V2 通用)'],
      ['编译日期(如 <code>Jul 23 2026</code>)', 'V1 APP 经 cmd:132 返回的 __DATE__'],
    ]) +
    '<h3>4.3 响应格式</h3>' +
    '<ul>' +
    '<li><strong>YSC 协议</strong>:JSON 帧 <code>{"code":200,"message":"&lt;name&gt;","data":"&lt;...&gt;"}</code>。<code>code:200</code> 表示成功;抖动/鼠标曲线的查询结果经异步事件 <code>debug_response</code>(message=<code>jitter</code> / <code>mouse_curve</code>)回传。</li>' +
    '<li><strong>MAKCU 协议</strong>:纯文本,响应以 <code>\\r\\n&gt;&gt;&gt;</code> 结尾。</li>' +
    '</ul>' +
    '<h3>4.4 串口参数</h3>' +
    '<p>数据位 8 / 停止位 1 / 无校验 / 无流控。波特率固件白名单:<code>115200 / 230400 / 460800 / 921600 / 1M / 1.5M / 2M / 3M</code>;<code>4M</code> 为实验性扩展(硬件不一定稳定)。可用 cmd:133 或 <code>km.baud</code> 动态切换。</p>';
}

function consistencySection() {
  return '' +
    '<h2>五、V1 / V2 文档一致性核对</h2>' +
    '<p>核对结论:<strong>两版命令集与协议语义一致</strong>,差异仅在物理传输与芯片架构。下表为逐项比对。</p>' +
    table(['项目', '8K V1(单芯片)', '8K V2(双芯片 / 8K V2)', '是否一致'], [
      ['命令编号', 'YSC cmd 30~44、50、132、133', '同左(命令号完全相同)', '<span class="badge">一致</span>'],
      ['MAKCU 协议', 'km.* 文本协议', '同左', '<span class="badge">一致</span>'],
      ['帧格式', '<code>&lt;START&gt;</code>[2B大端长度][JSON]<code>&lt;END&gt;</code>', '同左', '<span class="badge">一致</span>'],
      ['命令传输', 'USART1 UART(3M/4M 波特率)', 'USB-CDC(VID 1A86 / PID FE0C,波特率无关)', '语义一致,物理层不同'],
      ['芯片架构', '单芯片(USBHS Host + Device 同片)', '双芯片(Right=Host 读真设备 / Left=Device 对 PC,片间互联)', '架构不同(对上位机透明)'],
      ['IAP 升级', 'UART 1.5Mbaud', 'USB-CDC(序列号 TOWMCUIAP)', '语义一致,通道不同'],
      ['文档完整性', '—', '—', '见下'],
    ]) +
    '<div class="card">' +
    '<h4>本次核对修正项</h4>' +
    '<ul>' +
    '<li><strong>cmd 39~44(抖动 / 鼠标曲线)此前缺失</strong>:驱动 GUI(App.vue)已使用这些命令下发抖动与鼠标曲线配置,但协议文档(i18n/docs/{zh,en}.js)未收录。已补入 zh.js / en.js 的 YSC 命令列表,与本 PDF 保持一致。</li>' +
    '<li>cmd:50(IAP)、cmd:132(版本)、cmd:133(波特率)在 V1/V2 文档中描述一致,无需改动。</li>' +
    '<li>MAKCU 协议(km.*)两版完全一致。</li>' +
    '</ul></div>';
}

// ---- Main builder: turns the i18n `docs` object into a full HTML document ----

export function buildPdfHtml(docs, appVersion) {
  const d = new Date();
  const dateStr = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  let h = '<!DOCTYPE html><html lang="zh"><head><meta charset="utf-8">' +
          '<title>YSC 8K 使用与协议文档</title><style>' + CSS + '</style></head><body>';

  h += '<div class="cover"><h1>YSC 8K — 使用与协议文档</h1>' +
       '<div class="meta">驱动版本 ' + esc(appVersion || '') + ' · 生成日期 ' + dateStr +
       ' · 适用 8K V1(单芯片)与 8K V2(双芯片 / 8K V2)</div></div>';

  h += '<div class="toc"><strong>目录</strong><br>' +
       '<a>一、盒子使用</a>　·　<a>二、串口通信协议</a>　·　<a>三、命令详细参数</a>　·　' +
       '<a>四、设备状态与响应</a>　·　<a>五、V1 / V2 一致性核对</a></div>';

  // 一、使用
  h += usageSection();

  // 二、协议(来自 docs.serial)
  const s = docs.serial;
  h += '<h2>二、' + esc(s.title) + '</h2>';
  h += '<h3>' + esc(s.basicTitle) + '</h3>';
  h += '<table><tbody>';
  s.basicRows.forEach((r) => {
    h += '<tr><td class="kv">' + esc(r[0]) + '</td><td>' + r[1] + '</td></tr>';
  });
  h += '</tbody></table>';
  h += '<h3>' + esc(s.frameTitle) + '</h3>';
  h += '<pre>' + esc(s.frameFormat) + '</pre>';
  h += '<p class="note">' + s.frameNote + '</p>';
  // 命令分组(YSC / MAKCU)
  s.sections.forEach((sec) => {
    h += '<h3>' + esc(sec.title) + '</h3>';
    sec.cards.forEach((card) => {
      h += '<div class="card">';
      h += '<h4>' + esc(card.title) + '</h4>';
      if (card.isTable) {
        h += table(card.headers, card.rows);
      } else {
        if (card.format) h += '<pre>' + esc(card.format) + '</pre>';
        if (card.note) h += '<p class="note">' + card.note + '</p>';
      }
      h += '</div>';
    });
  });

  // 三、命令详细参数(来自 docs.protocols)
  h += '<h2>三、命令详细参数</h2>';
  docs.protocols.forEach((proto) => {
    h += '<h3>' + esc(proto.name) + '</h3>';
    proto.commands.forEach((cmd) => {
      h += '<div class="card">';
      h += '<h4>' + esc(cmd.name) + '</h4>';
      if (cmd.format) h += '<pre>' + esc(cmd.format) + '</pre>';
      if (cmd.note) h += '<p class="note">' + cmd.note + '</p>';
      if (cmd.params && cmd.params.length) {
        h += table(['参数', '类型', '说明'],
          cmd.params.map((p) => ['<code>' + esc(p.name) + '</code>', esc(p.type), esc(p.desc)]));
      }
      h += '</div>';
    });
  });

  // 四、状态
  h += statusSection();

  // 五、一致性
  h += consistencySection();

  h += '<p class="meta" style="margin-top:18pt;border-top:1px solid #ddd;padding-top:6pt">' +
       '本文档由 YSC 8K 驱动自动生成 · 命令语义以设备固件实现为准</p>';
  h += '</body></html>';
  return h;
}
