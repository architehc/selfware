### FILE: scripts/hist_importer.py
<<<<<<< SEARCH
def clean(history):
    """Clean up records from source database.

    Receives a list of record and sanityze them in order for them to be
    properly imported to qutebrowser. Sanitation requires adding a 4th
    attribute 'redirect' which is filled with '0's, and also purging all
    records that have a NULL/None datetime attribute.

    Args:
        history: List of records (datetime, url, title) from source database.
    """


    # replace missing titles with an empty string
    for index, record in enumerate(history):
        if record[1] is None:
            cleaned = list(record)
            cleaned[1] = ''
            history[index] = tuple(cleaned)

    nulls = [record for record in history if None in record]
    for null_record in nulls:
        history.remove(null_record)
    history = [list(record) for record in history]
    for record in history:
        record.append('0')
    return history
=======
def clean(history):
    """Clean up records from source database.

    Receives a list of record and sanityze them in order for them to be
    properly imported to qutebrowser. Sanitation requires adding a 4th
    attribute 'redirect' which is filled with '0's, and also purging all
    records that have a NULL/None datetime attribute.

    Args:
        history: List of records (datetime, url, title) from source database.
    """

    # Replace missing titles with an empty string
    for index, record in enumerate(history):
        if record[1] is None:
            cleaned = list(record)
            cleaned[1] = ''
            history[index] = tuple(cleaned)

    # Remove records with NULL/None datetime attribute
    history = [record for record in history if None not in record]

    # Add 'redirect' attribute with value '0'
    for record in history:
        record.append('0')

    return history
>>>>>>> REPLACE