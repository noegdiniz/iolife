import json

content = open('llm_responses.txt', encoding='utf-8').read()
entries = content.split('==== [')[1:]

print('=== Sample long_term_plan texts ===')
count = 0
for e in entries:
    try:
        op = e.split('operation=')[1].split(' ====')[0]
        if op == 'thought_generation':
            body = e.split('====\n', 1)[1].strip()
            data = json.loads(body)
            ltp = data.get('long_term_plan', '')
            ic = data.get('inner_contradiction_update', '')
            mf = data.get('melancholic_fixation', '')
            if ltp:
                print(f'  #{count+1} PLAN: "{ltp}"')
                if ic: print(f'       CONTRADICTION: "{ic}"')
                if mf: print(f'       MELANCHOLY: "{mf}"')
                print()
                count += 1
                if count >= 15:
                    break
    except Exception as ex:
        pass
