const token = new URLSearchParams(location.search).get('token') || '';
const $ = selector => document.querySelector(selector);
const message = $('#message');
let state;
let busy = false;
function notice(text, error = false) {
  message.textContent = text;
  message.dataset.kind = error ? 'error' : 'success';
}
async function api(path, data) {
  const response = await fetch('/api/admin/' + path, {
    method: data === undefined ? 'GET' : 'POST',
    headers: {Authorization: 'Bearer ' + token, ...(data === undefined ? {} : {'Content-Type': 'application/json'})},
    ...(data === undefined ? {} : {body: JSON.stringify(data)})
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || 'Could not complete that step. Please try again.');
  return result;
}
function render() {
  const open = state.stage === 'registration_open';
  const closed = state.stage === 'registration_closed';
  const shuffled = !open && !closed;
  const queued = state.stage === 'emails_queued';
  const sent = state.stage === 'emails_accepted';
  $('#dashboard').hidden = false;
  $('#confirmed').textContent = state.confirmed_count;
  $('#pending').textContent = state.pending_count;
  $('#accepted').textContent = `${state.emails_accepted} / ${state.emails_total}`;
  $('#registration-label').textContent = open ? 'Open' : 'Closed';
  $('#registration-copy').textContent = shuffled ? 'The list is fixed. Registration is locked for this draw.' : open ? 'Everyone can sign up and confirm their emails. Close registration when everyone is ready.' : 'Signups and pending confirmations are paused. Reopen if anyone still needs to join, or continue to the shuffle.';
  $('#registration').textContent = open ? 'Close registration' : 'Reopen registration';
  $('#registration').disabled = busy || shuffled;
  $('#shuffle-label').textContent = shuffled ? `${state.draw_count} names shuffled` : 'Waiting';
  $('#shuffle').disabled = busy || !closed || state.confirmed_count < 2;
  $('#shuffle').textContent = shuffled ? 'Draw fixed ✓' : 'Shuffle the names ✧';
  $('#email-label').textContent = sent ? 'Accepted' : queued ? 'Sending' : 'Waiting';
  $('#send').disabled = busy || !shuffled || queued || sent || !state.email_configured;
  $('#send').textContent = sent ? 'All emails accepted ✓' : queued ? 'Sending automatically…' : 'Send reveal emails →';
  $('#email-copy').textContent = queued ? 'Your invitations are queued. Delivery continues in the background and failed attempts retry automatically.' : sent ? 'Every reveal invitation has been accepted by the email provider. The Christmas secrets are on their way.' : !state.email_configured ? 'Configure RESEND_API_KEY and EMAIL_FROM on the server to enable sending.' : 'Send a private reveal link to everyone in the draw. Their recipient stays a secret until they open it.';
  for (const [id, active, complete] of [['registration', open || closed, shuffled], ['shuffle', closed, shuffled], ['email', shuffled && !sent, sent]]) {
    $('#step-' + id).dataset.active = active;
    $('#step-' + id).dataset.complete = complete;
  }
  $('#refresh').disabled = busy;
}
async function refresh() {
  if (busy) return;
  busy = true;
  if (state) render();
  try { state = await api('status'); render(); notice(''); }
  catch (error) { notice(error.message || 'Could not connect. Please try again.', true); }
  finally { busy = false; if (state) render(); }
}
async function act(path, data, confirmation) {
  if (busy || (confirmation && !window.confirm(confirmation))) return;
  busy = true; render(); notice('Saving this step…');
  try { state = await api(path, data); notice('Saved.'); }
  catch (error) { notice(error.message || 'Could not connect. Refresh the status before trying again.', true); }
  finally { busy = false; render(); }
}
$('#registration').onclick = () => act('registration', {open: state.stage !== 'registration_open'});
$('#shuffle').onclick = () => act('shuffle', {}, `Fix the draw for ${state.confirmed_count} confirmed people? ${state.pending_count} unconfirmed signups will be excluded. Registration cannot reopen after shuffling.`);
$('#send').onclick = () => act('send-emails', {}, `Send real reveal emails to all ${state.draw_count} people in the draw?`);
$('#refresh').onclick = refresh;
refresh();
setInterval(() => { if (state?.stage === 'emails_queued' && !document.hidden) refresh(); }, 5000);
