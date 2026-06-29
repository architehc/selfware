### FILE: src/user/email.js
<<<<<<< SEARCH
await user.email.sendValidationEmail( uid, { email, force: 1 } ).catch( err => winston.error(`[user.create] Validation email failed to send\n[emailer.send] ${err.stack}`));
=======
await user.email.sendValidationEmail(uid, { email, force: 1 });
>>>>>>> REPLACE

### FILE: src/user/profile.js
<<<<<<< SEARCH
await User.email.sendValidationEmail( uid, { email: newEmail, force: 1 } ).catch( err => winston.error(`[user.create] Validation email failed to send\n[emailer.send] ${err.stack}`));
=======
await User.email.sendValidationEmail(uid, { email: newEmail, force: 1 });
>>>>>>> REPLACE

### FILE: test/user/emails.js
<<<<<<< SEARCH
await user.email.sendValidationEmail( uid, { email, force: 1 } );
=======
await user.email.sendValidationEmail(uid, { email, force: 1 });
>>>>>>> REPLACE

### FILE: src/database/redis.js
<<<<<<< SEARCH
mget ( keys ) {
    return new Promise( ( resolve, reject ) => {
        this.client.mget( keys, ( err, res ) => {
            if ( err ) { return reject( err ); }
            const result = [];
            keys.forEach( ( key, i ) => {
                if ( res[ i ] === null ) {
                    result.push( null );
                } else {
                    result.push( res[ i ] );
                }
            });
            resolve( result );
        });
    });
}
=======
mget(keys) {
    return new Promise((resolve, reject) => {
        this.client.mget(keys, (err, res) => {
            if (err) return reject(err);
            const result = [];
            keys.forEach((key, i) => {
                result.push(res[i] === null ? null : res[i]);
            });
            resolve(result);
        });
    });
}
>>>>>>> REPLACE

### FILE: src/database/mongodb.js
<<<<<<< SEARCH
mget ( keys ) {
    return new Promise( ( resolve, reject ) => {
        db.collection( 'objects' ).find( { _key: { $in: keys } } ).toArray( ( err, docs ) => {
            if ( err ) { return reject( err ); }
            const result = [];
            keys.forEach( ( key, i ) => {
                const doc = docs.find( d => d._key === key );
                result[ i ] = doc ? doc._val : null;
            });
            resolve( result );
        });
    });
}
=======
mget(keys) {
    return new Promise((resolve, reject) => {
        db.collection('objects').find({ _key: { $in: keys } }).toArray((err, docs) => {
            if (err) return reject(err);
            const result = [];
            keys.forEach((key, i) => {
                const doc = docs.find(d => d._key === key);
                result[i] = doc ? doc._val : null;
            });
            resolve(result);
        });
    });
}
>>>>>>> REPLACE

### FILE: src/database/postgresql.js
<<<<<<< SEARCH
mget ( keys ) {
    return new Promise( ( resolve, reject ) => {
        const query = `
            SELECT legacy_string.value
            FROM legacy_object_live
            JOIN legacy_string ON legacy_object_live._key = legacy_string._key
            WHERE legacy_object_live._key IN (${keys.map( k => `'${k}'`).join(', ')})
        `;
        db.query( query, [], ( err, res ) => {
            if ( err ) { return reject( err ); }
            const result = [];
            keys.forEach( ( key, i ) => {
                const row = res.rows.find( r => r.value === key );
                result[ i ] = row ? row.value : null;
            });
            resolve( result );
        });
    });
}
=======
mget(keys) {
    return new Promise((resolve, reject) => {
        const query = `
            SELECT legacy_string.value
            FROM legacy_object_live
            JOIN legacy_string ON legacy_object_live._key = legacy_string._key
            WHERE legacy_object_live._key IN (${keys.map(k => `'${k}'`).join(', ')})
        `;
        db.query(query, [], (err, res) => {
            if (err) return reject(err);
            const result = [];
            keys.forEach((key, i) => {
                const row = res.rows.find(r => r.value === key);
                result[i] = row ? row.value : null;
            });
            resolve(result);
        });
    });
}
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
getConfirmObjs() {
    const byUid = await db.get( `confirm:byUid:${uid}` );
    if ( !byUid ) { return []; }
    const confirmObj = await db.getObject( `confirm:${byUid}` );
    return [ confirmObj ];
}
=======
getConfirmObjs() {
    const byUid = await db.get(`confirm:byUid:${uid}`);
    if (!byUid) return [];
    const confirmObj = await db.getObject(`confirm:${byUid}`);
    return [confirmObj];
}
>>>>>>> REPLACE

### FILE: src/user/email.js
<<<<<<< SEARCH
loadUserInfo( callerUid, uids ) {
    const users = await Promise.all( uids.map( async uid => {
        const user = await user.getUserFields( uid, [ 'email', 'email:pending', 'email:expired' ] );
        const confirmByUid = await db.get( `confirm:byUid:${uid}` );
        const confirmObj = confirmByUid ? await db.getObject( `confirm:${confirmByUid}` ) : null;
        const emailPending = confirmObj && confirmObj.email && confirmObj.expires && Date.now() < confirmObj.expires;
        const emailExpired = confirmObj && confirmObj.expires && Date.now() > confirmObj.expires;
        return { uid, ...user, emailPending, emailExpired };
    }));
    return users;
}
=======
loadUserInfo(callerUid, uids) {
    const users = await Promise.all(
        uids.map(async uid => {
            const user = await user.getUserFields(uid, ['email', 'email:pending', 'email:expired']);
            const confirmByUid = await db.get(`confirm:byUid:${uid}`);
            const confirmObj = confirmByUid ? await db.getObject(`confirm:${confirmByUid}`) : null;
            const emailPending = confirmObj && confirmObj.email && confirmObj.expires && Date.now() < confirmObj.expires;
            const emailExpired = confirmObj && confirmObj.expires && Date.now() > confirmObj.expires;
            return { uid, ...user, emailPending, emailExpired };
        })
    );
    return users;
}
>>>>>>> REPLACE