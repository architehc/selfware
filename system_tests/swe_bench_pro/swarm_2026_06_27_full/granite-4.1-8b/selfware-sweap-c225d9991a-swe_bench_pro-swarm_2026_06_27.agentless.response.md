### FILE: src/user/email.js
<<<<<<< SEARCH
await user.getUserField(uid, 'email');
=======
await user.email.getEmailForValidation(uid);
>>>>>>> REPLACE

### FILE: src/api/users.js
<<<<<<< SEARCH
await user.updateProfile(caller.uid, data);
=======
await user.email.confirmByUid(data.uid);
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
if (field === 'email') {
    return await updateEmail(updateUid, data.email);
}
=======
if (field === 'email') {
    await user.email.confirmByUid(updateUid);
}
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
await user.email.sendValidationEmail(uid, { email });
=======
await user.email.sendValidationEmail(uid, { email, force: 1 });
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
await user.email.isValidationPending(uid);
=======
await user.email.isValidationPending(uid, 'test@example.org');
>>>>>>> REPLACE

### FILE: test/database/keys.js
<<<<<<< SEARCH
it('should return multiple keys and null if key doesn\'t exist', async () => {
    const data = await db.mget(['doesnotexist', 'testKey']);
    assert.deepStrictEqual(data, [null, 'testValue']);
});
=======
it('should return multiple keys and null if key doesn\'t exist', async () => {
    const data = await db.mget(['doesnotexist', 'testKey']);
    assert.deepStrictEqual(data, [null, 'testValue']);
});
>>>>>>> REPLACE

### FILE: test/database/keys.js
<<<<<<< SEARCH
it('should return empty array if keys is empty array or falsy', async () => {
    assert.deepStrictEqual(await db.mget([]), []);
    assert.deepStrictEqual(await db.mget(false), []);
    assert.deepStrictEqual(await db.mget(null), []);
});
=======
it('should return empty array if keys is empty array or falsy', async () => {
    assert.deepStrictEqual(await db.mget([]), []);
    assert.deepStrictEqual(await db.mget(false), []);
    assert.deepStrictEqual(await db.mget(null), []);
});
>>>>>>> REPLACE

### FILE: src/database/redis/main.js
<<<<<<< SEARCH
// Existing mget implementation (if any)
=======
db.mget = async function (keys) {
    const results = await this.client.mget(...keys);
    return results.map((result, index) => result || null);
};
>>>>>>> REPLACE

### FILE: src/database/mongo/main.js
<<<<<<< SEARCH
// Existing mget implementation (if any)
=======
db.mget = async function (keys) {
    const cursor = this.collection.find({ _key: { $in: keys } });
    const docs = await cursor.toArray();
    const map = new Map(keys.map((k, i) => [k, docs[i] ? docs[i]._value : null]));
    return keys.map(k => map.get(k));
};
>>>>>>> REPLACE

### FILE: src/database/postgres/main.js
<<<<<<< SEARCH
// Existing mget implementation (if any)
=======
db.mget = async function (keys) {
    const placeholders = keys.map(() => '?').join(',');
    const query = `
        SELECT value
        FROM legacy_string
        WHERE key IN (${placeholders})
    `;
    const values = keys;
    const rows = await this.pool.query(query, values);
    return rows.rows.map(row => row.value || null);
};
>>>>>>> REPLACE