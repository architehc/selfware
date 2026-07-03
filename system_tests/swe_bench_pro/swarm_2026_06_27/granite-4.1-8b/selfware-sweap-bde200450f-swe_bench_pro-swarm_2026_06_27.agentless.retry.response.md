### FILE: src/user/email.js
<<<<<<< SEARCH
await db.set(`confirm:byUid:${uid}`, confirm_code);
await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1);
=======
await db.set(`confirm:byUid:${uid}`, confirm_code);
await db.pexpire(`confirm:byUid:${uid}`, emailConfirmExpiry * 60 * 60 * 1000);
>>>>>>> REPLACE

### FILE: src/user/emails.js
<<<<<<< SEARCH
await db. pexpire(`confirm:byUid:${uid}`, 1);
=======
await db.pexpire(`confirm:byUid:${uid}`, 1000);
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
await processDeletion({ uid, method: 'delete', password, caller });
=======
await processDeletion({ uid, method: 'delete', password, caller });
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
await User.setEmailFields(uid, { email: newEmail });
=======
await User.setEmailFields(uid, { email: newEmail });
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
await db. pexpire(`confirm:byUid:${uid}`, 1);
=======
await db.pexpire(`confirm:byUid:${uid}`, 1000);
>>>>>>> REPLACE

### FILE: test/user.js
<<<<<<< SEARCH
await user.email.sendValidationEmail( uid, { email, force: 1 });
=======
await user.email.sendValidationEmail( uid, { email, force: 1 });
>>>>>>> REPLACE