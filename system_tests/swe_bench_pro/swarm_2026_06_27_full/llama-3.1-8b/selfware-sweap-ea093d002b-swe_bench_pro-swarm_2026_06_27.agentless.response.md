### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);

	if (email) {
		const confirmObj = await db.getObject(`confirm:${code}`);
		return!!(confirmObj && email === confirmObj.email);
	}

	return!!code;
};
=======
UserEmail.isValidationPending = async (uid, email) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	const confirmObj = await db.getObject(`confirm:${code}`);
	return confirmObj && (email? email === confirmObj.email : true);
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.getValidationExpiry = async (uid) => {
	const pending = await UserEmail.isValidationPending(uid);
	return pending? db.pttl(`confirm:byUid:${uid}`) : null;
};
=======
UserEmail.getValidationExpiry = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	const confirmObj = await db.getObject(`confirm:${code}`);
	return confirmObj? confirmObj.expires : null;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.expireValidation = async (uid) => {
	const code = await db.get(`confirm:byUid:${uid}`);
	await db.deleteAll([
		`confirm:byUid:${uid}`,
		`confirm:${code}`,
	]);
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.canSendValidation = async (uid, email) => {
	const pending = await UserEmail.isValidationPending(uid, email);
	if (!pending) {
		return true;
	}

	const ttl = await UserEmail.getValidationExpiry(uid);
	const max = meta.config.emailConfirmExpiry * 60 * 60 * 1000;
	const interval = meta.config.emailConfirmInterval * 60 * 1000;

	return ttl + interval < max;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.sendValidationEmail = async function (uid, options) {
	/*
	 * Options:
	 * - email, overrides email retrieval
	 * - force, sends email even if it is too soon to send another
	 * - template, changes the template used for email sending
	 */

	if (meta.config.sendValidationEmail!== 1) {
		winston.verbose(`[user/email] Validation email for uid ${uid} not sent due to config settings`);
		return;
	}

	options = options || {};

	// Fallback behaviour (email passed in as second argument)
	if (typeof options === 'string') {
		options = {
			email: options,
		};
	}

	const confirm_code = utils.generateUUID();
const confirm_link = `${nconf.get('url')}/confirm/${confirm_code}`;

const { emailConfirmInterval, emailConfirmExpiry } = meta.config;

	// If no email passed in (default), retrieve email from uid
	if (!options.email ||!options.email.length) {
		options.email = await user.getUserField(uid, 'email');
	}
if (!options.email) {
		return;
}

if (!options.force &&!await UserEmail.canSendValidation(uid, options.email)) {
	throw new Error(`[[error:confirm-email-already-sent, ${emailConfirmInterval}]]`);
}

const username = await user.getUserField(uid, 'username');
const data = await plugins.hooks.fire('filter:user.verify', {
	uid,
	username,
	confirm_link,
	confirm_code: await plugins.hooks.fire('filter:user.verify.code', confirm_code),
	email: options.email,

	subject: options.subject || '[[email:email.verify-your-email.subject]]',
	template: options.template || 'verify-email',
});

await UserEmail.expireValidation(uid);
await db.set(`confirm:byUid:${uid}`, confirm_code);
await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1000);

await db.setObject(`confirm:${confirm_code}`, {
	email: options.email.toLowerCase(),
	uid: uid,
});
await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);

winston.verbose(`[user/email] Validation email for uid ${uid} sent to ${options.email}`);
events.log({
	type: 'email-confirmation-sent',
	uid,
	confirm_code,
	...options,
});

if (plugins.hooks.hasListeners('action:user.verify')) {
	plugins.hooks.fire('action:user.verify', { uid: uid, data: data });
} else {
	await emailer.send(data.template, uid, data);
}
return confirm_code;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.confirmByCode = async function (code, sessionId) {
	const confirmObj = await db.getObject(`confirm:${code}`);
if (!confirmObj ||!confirmObj.uid ||!confirmObj.email) {
	throw new Error('[[error:invalid-data]]');
}

	// If another uid has the same email, remove it
	const oldUid = await db.sortedSetScore('email:uid', confirmObj.email.toLowerCase());
if (oldUid) {
	await UserEmail.remove(oldUid, sessionId);
}

const oldEmail = await user.getUserField(confirmObj.uid, 'email');
if (oldEmail && confirmObj.email!== oldEmail) {
	await UserEmail.remove(confirmObj.uid, sessionId);
} else {
	await user.auth.revokeAllSessions(confirmObj.uid, sessionId);
}

await user.setUserField(confirmObj.uid, 'email', confirmObj.email);
await Promise.all([
	UserEmail.confirmByUid(confirmObj.uid),
	db.delete(`confirm:${code}`),
	events.log({ type: 'email-change', oldEmail, newEmail: confirmObj.email }),
]);
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.confirmByUid = async function (uid) {
if (!(parseInt(uid, 10) > 0)) {
	throw new Error('[[error:invalid-uid]]');
}
const currentEmail = await user.getUserField(uid, 'email');
if (!currentEmail) {
	throw new Error('[[error:invalid-email]]');
}

	// If another uid has the same email throw error
const oldUid = await db.sortedSetScore('email:uid', currentEmail.toLowerCase());
if (oldUid && oldUid!== parseInt(uid, 10)) {
	throw new Error('[[error:email-taken]]');
}

const confirmedEmails = await db.getSortedSetRangeByScore(`email:uid`, 0, -1, uid, uid);
if (confirmedEmails.length) {
	// remove old email of user by uid
	await db.sortedSetsRemoveRangeByScore([`email:uid`], uid, uid);
	await db.sortedSetRemoveBulk(
		confirmedEmails.map(email => [`email:sorted`, `${email.toLowerCase()}:${uid}`])
	);
}
await Promise.all([
	db.sortedSetAddBulk([
		['email:uid', uid, currentEmail.toLowerCase()],
		['email:sorted', 0, `${currentEmail.toLowerCase()}:${uid}`],
		[`user:${uid}:emails`, Date.now(), `${currentEmail}:${Date.now()}`],
	]),
	user.setUserField(uid, 'email:confirmed', 1),
	groups.join('verified-users', uid),
	groups.leave('unverified-users', uid),
	user.email.expireValidation(uid),
	user.reset.cleanByUid(uid),
]);
await plugins.hooks.fire('action:user.email.confirmed', { uid: uid, email: currentEmail });
};
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
usersAPI.update = async function (caller, data) {
if (!caller.uid) {
	throw new Error('[[error:invalid-uid]]');
}

if (!data ||!data.uid) {
	throw new Error('[[error:invalid-data]]');
}

const oldUserData = await user.getUserFields(data.uid, ['email', 'username']);
if (!oldUserData ||!oldUserData.username) {
	throw new Error('[[error:invalid-data]]');
}

const [isAdminOrGlobalMod, canEdit] = await Promise.all([
	user.isAdminOrGlobalMod(caller.uid),
	privileges.users.canEdit(caller.uid, data.uid),
]);

	// Changing own email/username requires password confirmation
if (data.hasOwnProperty('email') || data.hasOwnProperty('username')) {
	await isPrivilegedOrSelfAndPasswordMatch(caller, data);
}

if (!canEdit) {
	throw new Error('[[error:no-privileges]]');
}

if (!isAdminOrGlobalMod && meta.config['username:disableEdit']) {
data.username = oldUserData.username;
}

if (!isAdminOrGlobalMod && meta.config['email:disableEdit']) {
data.email = oldUserData.email;
}

await user.updateProfile(caller.uid, data);
const userData = await user.getUserData(data.uid);

if (userData.username!== oldUserData.username) {
await events.log({
	type: 'username-change',
	uid: caller.uid,
	targetUid: data.uid,
	ip: caller.ip,
	oldUsername: oldUserData.username,
	newUsername: userData.username,
});
}
return userData;
};
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
User.updateProfile = async function (uid, data, extraFields) {
let fields = [
	'username', 'email', 'fullname', 'website', 'location',
	'groupTitle', 'birthday', 'signature', 'aboutme',
];
if (Array.isArray(extraFields)) {
	fields = _.uniq(fields.concat(extraFields));
}
if (!data.uid) {
	throw new Error('[[error:invalid-update-uid]]');
}
const updateUid = data.uid;

const result = await plugins.hooks.fire('filter:user.updateProfile', {
	uid: uid,
	data: data,
	fields: fields,
});
fields = result.fields;
data = result.data;

await validateData(uid, data);

const oldData = await User.getUserFields(updateUid, fields);
const updateData = {};
await Promise.all(fields.map(async (field) => {
if (!(data[field]!== undefined && typeof data[field] === 'string')) {
	return;
}

data[field] = data[field].trim();

if (field === 'email') {
return await updateEmail(updateUid, data.email);
} else if (field === 'username') {
return await updateUsername(updateUid, data.username);
} else if (field === 'fullname') {
return await updateFullname(updateUid, data.fullname);
}
updateData[field] = data[field];
}));

if (Object.keys(updateData).length) {
await User.setUserFields(updateUid, updateData);
}

plugins.hooks.fire('action:user.updateProfile', {
	uid: uid,
	data: data,
	fields: fields,
	oldData: oldData,
});

return await User.getUserFields(updateUid, [
	'email', 'username', 'userslug',
	'picture', 'icon:text', 'icon:bgColor',
]);
};

async function validateData(callerUid, data) {
await isEmailValid(data);
await isUsernameAvailable(data, data.uid);
await isWebsiteValid(callerUid, data);
await isAboutMeValid(callerUid, data);
await isSignatureValid(callerUid, data);
isFullnameValid(data);
isLocationValid(data);
isBirthdayValid(data);
isGroupTitleValid(data);
}

async function isEmailValid(data) {
if (!data.email) {
	return;
}

data.email = data.email.trim();
if (!utils.isEmailValid(data.email)) {
throw new Error('[[error:invalid-email]]');
}
}

async function isUsernameAvailable(data, uid) {
if (!data.username) {
	return;
}
data.username = data.username.trim();

let userData;
if (uid) {
userData = await User.getUserFields(uid, ['username', 'userslug']);
if (userData.username === data.username) {
	return;
}
}

if (data.username.length < meta.config.minimumUsernameLength) {
throw new Error('[[error:username-too-short]]');
}

if (data.username.length > meta.config.maximumUsernameLength) {
throw new Error('[[error:username-too-long]]');
}

const userslug = slugify(data.username);
if (!utils.isUserNameValid(data.username) ||!userslug) {
throw new Error('[[error:invalid-username]]');
}

if (uid && userslug === userData.userslug) {
	return;
}
const exists = await User.existsBySlug(userslug);
if (exists) {
throw new Error('[[error:username-taken]]');
}

const { error } = await plugins.hooks.fire('filter:username.check', {
	username: data.username,
	error: undefined,
});
if (error) {
throw error;
}
}
User.checkUsername = async username => isUsernameAvailable({ username });

async function isWebsiteValid(callerUid, data) {
if (!data.website) {
	return;
}
if (data.website.length > 255) {
throw new Error('[[error:invalid-website]]');
}
await User.checkMinReputation(callerUid, data.uid, 'min:rep:website');
}

async function isAboutMeValid(callerUid, data) {
if (!data.aboutme) {
	return;
}
if (data.aboutme!== undefined && data.aboutme.length > meta.config.maximumAboutMeLength) {
throw new Error(`[[error:about-me-too-long, ${meta.config.maximumAboutMeLength}]]`);
}

await User.checkMinReputation(callerUid, data.uid, 'min:rep:aboutme');
}

async function isSignatureValid(callerUid, data) {
if (!data.signature) {
	return;
}
if (data.signature!== undefined && data.signature.length > meta.config.maximumSignatureLength) {
throw new Error(`[[error:signature-too-long, ${meta.config.maximumSignatureLength}]]`);
}

await User.checkMinReputation(callerUid, data.uid, 'min:rep:signature');
}

function isFullnameValid(data) {
if (data.fullname && (validator.isURL(data.fullname) || data.fullname.length > 255)) {
throw new Error('[[error:invalid-fullname]]');
}
}

function isLocationValid(data) {
if (data.location && (validator.isURL(data.location) || data.location.length > 255)) {
throw new Error('[[error:invalid-location]]');
}
}

function isBirthdayValid(data) {
if (!data.birthday) {
	return;
}

const result = new Date(data.birthday);
if (result && result.toString() === 'Invalid Date') {
throw new Error('[[error:invalid-birthday]]');
}
}

function isGroupTitleValid(data) {
function checkTitle(title) {
if (title === 'registered-users' || groups.isPrivilegeGroup(title)) {
throw new Error('[[error:invalid-group-title]]');
}
}

if (!data.groupTitle) {
	return;
}
let groupTitles = [];
if (validator.isJSON(data.groupTitle)) {
groupTitles = JSON.parse(data.groupTitle);
if (!Array.isArray(groupTitles)) {
throw new Error('[[error:invalid-group-title]]');
}
groupTitles.forEach(title => checkTitle(title));
} else {
groupTitles = [data.groupTitle];
checkTitle(data.groupTitle);
}
if (!meta.config.allowMultipleBadges && groupTitles.length > 1) {
data.groupTitle = JSON.stringify(groupTitles[0]);
}
}

User.checkMinReputation = async function (callerUid, uid, setting) {
const isSelf = parseInt(callerUid, 10) === parseInt(uid, 10);
if (!isSelf || meta.config['reputation:disabled']) {
	return;
}
const reputation = await User.getUserField(uid, 'reputation');
if (reputation < meta.config[setting]) {
throw new Error(`[[error:not-enough-reputation-${setting.replace(/:/g, '-')}, ${meta.config[setting]}]]`);
}
};

async function updateEmail(uid, newEmail) {
let oldEmail = await User.getUserField(uid, 'email');
oldEmail = oldEmail || '';
if (oldEmail === newEmail) {
	return;
}

	// 👉 Looking for email change logic? src/user/email.js (UserEmail.confirmByUid)
if (newEmail) {
await User.email.sendValidationEmail(uid, {
	email: newEmail,
	force: 1,
}).catch(err => winston.error(`[user.create] Validation email failed to send\n[emailer.send] ${err.stack}`));
}
}

async function updateUsername(uid, newUsername) {
if (!newUsername) {
	return;
}
const userData = await User.getUserFields(uid, ['username', 'userslug']);
if (userData.username === newUsername) {
	return;
}
const newUserslug = slugify(newUsername);
const now = Date.now();
await Promise.all([
updateUidMapping('username', uid, newUsername, userData.username),
updateUidMapping('userslug', uid, newUserslug, userData.userslug),
db.sortedSetAdd(`user:${uid}:usernames`, now, `${newUsername}:${now}`),
]);
await db.sortedSetRemove('username:sorted', `${userData.username.toLowerCase()}:${uid}`);
await db.sortedSetAdd('username:sorted', 0, `${newUsername.toLowerCase()}:${uid}`);
}

async function updateUidMapping(field, uid, value, oldValue) {
if (value === oldValue) {
	return;
}
await db.sortedSetRemove(`${field}:uid`, oldValue);
await User.setUserField(uid, field, value);
if (value) {
await db.sortedSetAdd(`${field}:uid`, uid, value);
}
}

async function updateFullname(uid, newFullname) {
const fullname = await User.getUserField(uid, 'fullname');
await updateUidMapping('fullname', uid, newFullname, fullname);
if (newFullname!== fullname) {
if (fullname) {
await db.sortedSetRemove('fullname:sorted', `${fullname.toLowerCase()}:${uid}`);
}
if (newFullname) {
await db.sortedSetAdd('fullname:sorted', 0, `${newFullname.toLowerCase()}:${uid}`);
}
}
}

User.changePassword = async function (uid, data) {
if (uid <= 0 ||!data ||!data.uid) {
throw new Error('[[error:invalid-uid]]');
}
User.isPasswordValid(data.newPassword);
const [isAdmin, hasPassword] = await Promise.all([
User.isAdministrator(uid),
User.hasPassword(uid),
]);

if (meta.config['password:disableEdit'] &&!isAdmin) {
throw new Error('[[error:no-privileges]]');
}

const isSelf = parseInt(uid, 10) === parseInt(data.uid, 10);

if (!isAdmin &&!isSelf) {
throw new Error('[[user:change_password_error_privileges]]');
}

if (isSelf && hasPassword) {
const correct = await User.isPasswordCorrect(data.uid, data.currentPassword, data.ip);
if (!correct) {
throw new Error('[[user:change_password_error_wrong_current]]');
}
}
>>>>>>> REPLACE