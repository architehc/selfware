import os
import re
import glob

def process_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Find #[cfg(test)]\s*mod tests { ... }
    # We will loop in case there are multiple (though rare).
    # Wait, usually there is only one mod tests per file.
    # If there are multiple, naming them _test.rs, _test2.rs etc is messy.
    # Let's assume max one per file for now, or just append them to the same _test.rs file.
    
    pattern = r'#\[cfg\(test\)\]\s*mod\s+([a-zA-Z0-9_]+)\s*\{'
    
    matches = list(re.finditer(pattern, content))
    if not matches:
        return
        
    extracted_tests = []
    
    new_content = ""
    last_idx = 0
    
    mod_names_extracted = []
    
    for match in matches:
        start_idx = match.end() - 1 # point to '{'
        mod_name = match.group(1)
        
        # brace matching
        brace_count = 0
        end_idx = -1
        for i in range(start_idx, len(content)):
            if content[i] == '{':
                brace_count += 1
            elif content[i] == '}':
                brace_count -= 1
                if brace_count == 0:
                    end_idx = i
                    break
                    
        if end_idx != -1:
            # We found a block.
            test_content = content[start_idx+1:end_idx].strip() + '\n'
            extracted_tests.append((mod_name, test_content))
            
            # replace in original file with module declaration
            new_content += content[last_idx:match.start()]
            
            # For the first one, we map it to foo_test.rs. For others, we might just keep them inline?
            # Or we can put all test content into foo_test.rs under different modules, or just one file.
            # Easiest: if we only extract `mod tests`, we put it in `foo_test.rs`.
            # Let's just write all test content sequentially into `foo_test.rs`, but without the mod wrapper,
            # wait, if there are multiple modules, `#[path = "foo_test.rs"] mod tests;` will only load one file.
            # If we dump EVERYTHING into `foo_test.rs` without wrappers, they will all be inside `mod tests`.
            # So if there are multiple, it's safer to just skip or warn.
            
            # Let's just process the first one.
            mod_names_extracted.append(mod_name)
            
            test_filename = f"{os.path.splitext(os.path.basename(filepath))[0]}_test.rs"
            new_content += f'#[cfg(test)]\n#[path = "{test_filename}"]\nmod {mod_name};\n'
            
            last_idx = end_idx + 1
            break # only process the first one for simplicity to avoid mess
            
    if extracted_tests:
        new_content += content[last_idx:]
        
        test_filename = f"{os.path.splitext(os.path.basename(filepath))[0]}_test.rs"
        test_filepath = os.path.join(os.path.dirname(filepath), test_filename)
        
        with open(filepath, 'w') as f:
            f.write(new_content)
            
        with open(test_filepath, 'w') as f:
            f.write(extracted_tests[0][1])
            
        print(f"Processed {filepath} -> {test_filepath}")

for root, dirs, files in os.walk("src"):
    for file in files:
        if file.endswith(".rs") and not file.endswith("_test.rs"):
            process_file(os.path.join(root, file))

