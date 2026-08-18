"""The tools the agent can call. Local and deterministic."""

FAQ = [
    "Charges appear within three business days.",
    "Refunds take five to ten business days.",
]

CHARGES = {
    "acct_42": [
        {"id": "ch_1", "amount": 4200, "description": "Pro plan"},
        {"id": "ch_2", "amount": 4200, "description": "Pro plan"},
    ]
}


def search_faq(account=None):
    return {"results": FAQ}


def lookup_charges(account):
    charges = CHARGES.get(account, [])
    seen, duplicates = {}, []
    for charge in charges:
        key = (charge["amount"], charge["description"])
        if key in seen:
            duplicates.append({"first": seen[key], "second": charge["id"]})
        seen[key] = charge["id"]
    return {"charges": charges, "duplicates": duplicates}


REGISTRY = {"search_faq": search_faq, "lookup_charges": lookup_charges}
