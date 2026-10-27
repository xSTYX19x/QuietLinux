// QuietLinux WebHID logger.
//
// Logs every HID report a web page exchanges with HID devices, to work out the
// protocol of a web configurator (IO Center Web). See docs/capturing.md.
//
// Usage: open the web app, open DevTools (F12) > Console, paste this file and
// press Enter BEFORE connecting the keyboard in the app. Then:
//   qlNote("brightness 50")   write a note into the log (what you are about to do)
//   qlSave("brightness")      download the log as webhid-log-brightness.txt
//   qlClear()                 empty the log
//
// The log stays in this browser tab until you save it; nothing is sent anywhere.
// Decode it with: qlcap frames webhid-log-brightness.txt
//
// Privacy: key presses can show up in the log (the keyboard reports them on its
// vendor interface). Do not type passwords while logging.
//
// Log format (one line per event):
//   <ms> <out|in|feature-out|feature-in> <vid:pid> <report id> <hex data>
//   <ms> note <text>

(() => {
  if (window.qlSave) {
    console.log('QuietLinux WebHID logger is already active.');
    return;
  }
  const log = [];
  const start = performance.now();
  const time = () => (performance.now() - start).toFixed(3);
  const hex4 = (n) => n.toString(16).padStart(4, '0');
  const id = (device) => `${hex4(device.vendorId)}:${hex4(device.productId)}`;
  const bytes = (data) =>
    data instanceof ArrayBuffer
      ? new Uint8Array(data)
      : new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  const hex = (data) =>
    Array.from(bytes(data), (b) => b.toString(16).padStart(2, '0')).join('');
  const add = (kind, device, reportId, data) =>
    log.push(`${time()} ${kind} ${id(device)} ${reportId} ${hex(data)}`);

  const proto = HIDDevice.prototype;

  const sendReport = proto.sendReport;
  proto.sendReport = function (reportId, data) {
    add('out', this, reportId, data);
    return sendReport.call(this, reportId, data);
  };

  const sendFeatureReport = proto.sendFeatureReport;
  proto.sendFeatureReport = function (reportId, data) {
    add('feature-out', this, reportId, data);
    return sendFeatureReport.call(this, reportId, data);
  };

  const receiveFeatureReport = proto.receiveFeatureReport;
  proto.receiveFeatureReport = async function (reportId) {
    const data = await receiveFeatureReport.call(this, reportId);
    add('feature-in', this, reportId, data);
    return data;
  };

  const listening = new WeakSet();
  const listen = (device) => {
    if (listening.has(device)) return;
    listening.add(device);
    device.addEventListener('inputreport', (e) => add('in', e.device, e.reportId, e.data));
  };

  const open = proto.open;
  proto.open = async function () {
    const result = await open.call(this);
    listen(this);
    return result;
  };
  // Devices the page opened before the logger was pasted.
  navigator.hid.getDevices().then((devices) => devices.filter((d) => d.opened).forEach(listen));

  window.qlNote = (text) => {
    log.push(`${time()} note ${String(text).replace(/\s+/g, ' ')}`);
  };
  window.qlClear = () => {
    log.length = 0;
  };
  window.qlSave = (name = 'capture') => {
    const text = `# quietlinux webhid log v1\n${log.join('\n')}\n`;
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
    link.download = `webhid-log-${name}.txt`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(link.href), 10000);
    console.log(`QuietLinux: saved ${log.length} lines.`);
  };
  console.log('QuietLinux WebHID logger active: qlNote("…"), qlSave("name"), qlClear()');
})();
