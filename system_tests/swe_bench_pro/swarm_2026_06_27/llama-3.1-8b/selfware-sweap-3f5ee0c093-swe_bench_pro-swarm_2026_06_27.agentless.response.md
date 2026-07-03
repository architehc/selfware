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
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
if (!options.force &&!await UserEmail.canSendValidation(uid, options.email)) {
	throw new Error(`[[error:confirm-email-already-sent, ${emailConfirmInterval}]]`);
}
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
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
const [isAdminOrGlobalMod, canEdit] = await Promise.all([
	user.isAdminOrGlobalMod(caller.uid),
	privileges.users.canEdit(caller.uid, data.uid),
]);

	// Changing own email/username requires password confirmation
if (data.hasOwnProperty('email') || data.hasOwnProperty('username')) {
	await isPrivilegedOrSelfAndPasswordMatch(caller, data);
}
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
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
const result = await plugins.hooks.fire('filter:user.updateProfile', {
	uid: uid,
	data: data,
	fields: fields,
});
fields = result.fields;
data = result.data;
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
await validateData(uid, data);
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
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
>>>>>>> REPLACE