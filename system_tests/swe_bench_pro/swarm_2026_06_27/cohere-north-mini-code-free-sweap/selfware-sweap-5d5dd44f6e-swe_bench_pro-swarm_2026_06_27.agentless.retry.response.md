### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.isValidationPending = async (uid, email) => {
    const code = await db.get(`confirm:byUid:${uid}`);

    if (email) {
        const confirmObj = await db.getObject(`confirm:${code}`);
        return !!(confirmObj && email === confirmObj.email);
    }

    return !!code;
};
=======
UserEmail.isValidationPending = async (uid, email) => {
    const code = await db.get(`confirm:byUid:${uid}`);
    if (!code) {
        return false;
    }
    const confirmObj = await db.getObject(`confirm:${code}`);
    if (!confirmObj || !confirmObj.uid || confirmObj.uid !== uid) {
        return false;
    }
    // Check expires
    const now = Date.now();
    if (confirmObj.expires && now >= confirmObj.expires) {
        return false;
    }
    if (email && confirmObj.email !== email) {
        return false;
    }
    return true;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
UserEmail.getValidationExpiry = async (uid) => {
    const pending = await UserEmail.isValidationPending(uid);
    return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
};
=======
UserEmail.getValidationExpiry = async (uid) => {
    const pending = await UserEmail.isValidationPending(uid);
    if (!pending) {
        return null;
    }
    const code = await db.get(`confirm:byUid:${uid}`);
    const confirmObj = await db.getObject(`confirm:${code}`);
    if (!confirmObj || !confirmObj.expires) {
        return null;
    }
    const remaining = confirmObj.expires - Date.now();
    return remaining > 0 ? remaining : null;
};
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
await db.set(`confirm:byUid:${uid}`, confirm_code);
await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1000);
=======
await db.set(`confirm:byUid:${uid}`, confirm_code);
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
await db.setObject(`confirm:${confirm_code}`, {
    email: options.email.toLowerCase(),
    uid: uid,
});
await db.pexpire(`confirm:${confirm_code}`, emailConfirmExpiry * 60 * 60 * 1000);
=======
const expires = Date.now() + emailConfirmExpiry * 60 * 60 * 1000;
await db.setObject(`confirm:${confirm_code}`, {
    email: options.email.toLowerCase(),
    uid: uid,
    expires,
});
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
await plugins.hooks.fire('action:user.email.confirmed', { uid: uid, email: currentEmail });
};
=======
await plugins.hooks.fire('action:user.email.confirmed', { uid: uid, email: currentEmail });
};

UserEmail.getEmailForValidation = async function (uid) {
    // First try to retrieve email from user's profile (user:<uid>)
    const email = await user.getUserField(uid, 'email');
    if (email) {
        return email;
    }
    // If no email is set, fallback to the email field in the confirmation object (confirm:<code>) corresponding to confirm:byUid:<uid>
    const code = await db.get(`confirm:byUid:${uid}`);
    if (!code) {
        return null;
    }
    const confirmObj = await db.getObject(`confirm:${code}`);
    if (confirmObj && confirmObj.uid === uid) {
        return confirmObj.email;
    }
    return null;
};

UserEmail.validateEmail = async function (uid) {
    const email = await UserEmail.getEmailForValidation(uid);
    if (email) {
        await user.setUserField(uid, 'email', email);
        await UserEmail.confirmByUid(uid);
    } else {
        throw new Error('[[error:invalid-data]]');
    }
};
>>>>>>> REPLACE